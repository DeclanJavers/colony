"""Colony brain server: Laya for decisions, a local LLM (LM Studio) for talking.

Godot calls this over localhost HTTP. Endpoints:
  GET  /health                  -> what's loaded
  POST /decide  {requests:[ctx]} -> {results:[decision]}   (Laya)
  POST /chat    {colonist, history, message} -> {reply}     (LLM)
  POST /remember {colonist, transcript} -> {memory}         (LLM: one-sentence memory)

A request ctx is exactly what Sim.build_context() produces in Godot. Swapping the decision
model (e.g. a fine-tuned Laya checkpoint) is a matter of LAYA_MODEL / LAYA_SUBFOLDER env vars.
"""

from __future__ import annotations

import os
import re
import threading
import time
from typing import Any

import httpx
from fastapi import FastAPI
from pydantic import BaseModel

from laya_format import QUESTIONS_VERSION, build_questions, label_to_id

LAYA_MODEL = os.environ.get("LAYA_MODEL", "convaiinnovations/laya")
LAYA_SUBFOLDER = os.environ.get("LAYA_SUBFOLDER") or None
LAYA_DEVICE = os.environ.get("LAYA_DEVICE") or None  # None = auto (mps on Apple silicon)
LLM_URL = os.environ.get("LLM_URL", "http://127.0.0.1:8766/v1/chat/completions")
LLM_MODEL = os.environ.get("LLM_MODEL", "lfm3-1.7b")

app = FastAPI(title="colony-brain")
_agent = None
_agent_lock = threading.Lock()
_agent_info: dict[str, Any] = {}


def agent():
    global _agent
    if _agent is None:
        import laya  # heavy import, only when first needed

        t = time.time()
        _agent = laya.load(LAYA_MODEL, subfolder=LAYA_SUBFOLDER, device=LAYA_DEVICE)
        _agent_info.update(model=LAYA_MODEL, subfolder=LAYA_SUBFOLDER, device=str(_agent.device),
                           load_seconds=round(time.time() - t, 1))
    return _agent


# ---------------------------------------------------------------------------- decisions

class DecideRequest(BaseModel):
    requests: list[dict[str, Any]]


def _to_ids(probs: dict[str, float], opts: list[dict]) -> dict[str, float]:
    by_label = label_to_id(opts)
    return {by_label[k]: float(v) for k, v in probs.items() if k in by_label}


def decide_one(ctx: dict) -> dict:
    opts, bopts, sopts = ctx["options"], ctx.get("build_options", []), ctx.get("social_options", [])
    questions = build_questions(ctx)

    t = time.time()
    with _agent_lock:
        res = agent().predict(ctx["state_text"], questions)
    ms = (time.time() - t) * 1000.0
    ans = res["answers"]

    probs = _to_ids(ans["action"]["probabilities"], opts)
    action = max(probs, key=probs.get) if probs else ""
    out = {"action": action, "probs": probs, "build_what": "", "build_probs": {},
           "with_whom": -1, "social_probs": {}, "brain": "laya", "latency_ms": ms,
           "model": _agent_info.get("model"), "questions_version": QUESTIONS_VERSION, "confidence": ans["action"].get("confidence")}
    if "build_what" in ans:
        bp = _to_ids(ans["build_what"]["probabilities"], bopts)
        out["build_probs"] = bp
        out["build_what"] = max(bp, key=bp.get) if bp else ""
    if "with_whom" in ans:
        sp = _to_ids(ans["with_whom"]["probabilities"], sopts)
        out["social_probs"] = sp
        out["with_whom"] = int(max(sp, key=sp.get)) if sp else -1
    out["truncated"] = res.get("usage", {}).get("truncated", False)
    return out


@app.post("/decide")
def decide(req: DecideRequest) -> dict:
    results = []
    for ctx in req.requests:
        try:
            r = decide_one(ctx)
        except Exception as e:  # never take the sim down; report the error per request
            r = {"error": repr(e), "action": "", "probs": {}, "brain": "laya"}
        r["request_id"] = ctx.get("request_id")
        results.append(r)
    return {"results": results}


# ---------------------------------------------------------------------------- talking

THINK_RE = re.compile(r"<think>.*?</think>", re.DOTALL)


def llm(messages: list[dict], max_tokens: int = 300, temperature: float = 0.6,
        stop: list[str] | None = None) -> str:
    body = {"model": LLM_MODEL, "messages": messages, "max_tokens": max_tokens,
            "temperature": temperature, "top_k": 50, "repeat_penalty": 1.1,
            "chat_template_kwargs": {"enable_thinking": False}}
    if stop:
        body["stop"] = stop
    r = httpx.post(LLM_URL, json=body, timeout=120.0)
    r.raise_for_status()
    text = r.json()["choices"][0]["message"]["content"] or ""
    text = THINK_RE.sub("", text)
    if "<think>" in text:  # unterminated reasoning: keep what follows it, if anything
        text = text.split("</think>")[-1] if "</think>" in text else ""
    return text.strip()


def persona(c: dict) -> str:
    needs = ", ".join(c.get("need_words", []))
    mems = "\n".join(f"- {m}" for m in c.get("memories", []))
    why = c.get("why", "")
    return (
        f"You are {c['name']}, a {c.get('stage', 'adult')} living in a small colony where everyone "
        f"shares everything. Your traits: {', '.join(c.get('traits', [])) or 'none'}.\n"
        f"How you feel right now: {needs}. These are your real feelings: never claim to be hungry, "
        f"thirsty, tired or lonely unless it says so here.\n"
        f"What you are doing: {c.get('action', 'idle')}. {why}\n"
        f"The colony: {c.get('colony', '')}\n"
        f"Things you remember:\n{mems or '- nothing much yet'}\n\n"
        "You are talking to the Watcher, a mysterious voice only you can hear. Stay in character. "
        "Reply with only what you say out loud: 1-3 short sentences. No actions in asterisks, no "
        "narration, no names, labels or formatting. "
        "Never mention being an AI or a game."
    )


SPEAKER_RE = re.compile(r"^\s*(\*\*)?[A-Z][\w ]{0,20}(\*\*)?\s*:")


def clean_reply(text: str, name: str) -> str:
    """Keep only the colonist's own words: small models sometimes start writing the next turn."""
    text = re.sub(r"\*[^*\n]{1,120}\*", " ", text.replace("**", "").replace("__", ""))  # *actions*
    text = text.strip()
    text = re.sub(rf"^{re.escape(name)}\s*:\s*", "", text)
    kept = []
    for line in text.splitlines():
        if kept and SPEAKER_RE.match(line):
            break
        if line.strip():
            kept.append(line.strip())
    out = " ".join(kept)
    out = re.sub(r"\s+", " ", out).strip()
    sentences = re.split(r"(?<=[.!?])\s+", out)
    # Drop third-person narration about the Watcher, then keep at most three sentences.
    sentences = [x for x in sentences if not re.search(r"\bThe Watcher (doesn't|does not|says|waits|is)\b", x)]
    return " ".join(sentences[:3]).strip().strip('"')


class ChatRequest(BaseModel):
    colonist: dict[str, Any]
    history: list[dict[str, str]] = []
    message: str


@app.post("/chat")
def chat(req: ChatRequest) -> dict:
    msgs = [{"role": "system", "content": persona(req.colonist)}]
    msgs += req.history[-10:]
    msgs.append({"role": "user", "content": req.message})
    t = time.time()
    name = req.colonist.get("name", "")
    try:
        reply = clean_reply(llm(msgs, max_tokens=160, stop=["\nWatcher:", "\nYou:", f"\n{name}:", f"\n**{name}"]), name)
    except Exception as e:
        return {"reply": "", "error": repr(e)}
    return {"reply": reply or "...", "latency_ms": (time.time() - t) * 1000.0}


class RememberRequest(BaseModel):
    colonist: dict[str, Any]
    transcript: list[dict[str, str]]


@app.post("/remember")
def remember(req: RememberRequest) -> dict:
    name = req.colonist.get("name", "I")
    convo = "\n".join(f"{'Watcher' if m['role'] == 'user' else name}: {m['content']}" for m in req.transcript)
    msgs = [
        {"role": "system", "content": "You compress conversations into one short first-person memory."},
        {"role": "user", "content": (
            f"{name} just had this conversation with the Watcher:\n{convo}\n\n"
            f"Write ONE short sentence (at most 15 words) in first person, as {name}, of what matters most "
            "to remember from it, especially any request or advice about what to do. "
            "Example: The Watcher asked me to fetch water because the colony is running low. "
            "Only the sentence.")},
    ]
    try:
        mem = llm(msgs, max_tokens=60, temperature=0.2).replace("**", "").split("\n")[0].strip().strip('"')
        words = mem.split()
        if len(words) > 25:  # memories go into Laya's ~320-token state, so keep them short
            mem = " ".join(words[:25]).rstrip(",;") + "..."
    except Exception as e:
        return {"memory": "", "error": repr(e)}
    return {"memory": mem}


# ---------------------------------------------------------------------------- health

@app.get("/health")
def health() -> dict:
    llm_ok = False
    try:
        llm_ok = httpx.get(LLM_URL.replace("/chat/completions", "/models"), timeout=2.0).status_code == 200
    except Exception:
        pass
    return {"ok": True, "laya_loaded": _agent is not None, "laya": _agent_info, "llm_reachable": llm_ok,
            "llm_model": LLM_MODEL}


@app.on_event("startup")
def _warm() -> None:
    if os.environ.get("LAYA_LAZY") != "1":
        agent()
