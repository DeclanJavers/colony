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
import json
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
        stop: list[str] | None = None, schema: dict | None = None) -> str:
    body = {"model": LLM_MODEL, "messages": messages, "max_tokens": max_tokens,
            "temperature": temperature, "top_k": 50, "repeat_penalty": 1.1,
            "chat_template_kwargs": {"enable_thinking": False}}
    if stop:
        body["stop"] = stop
    if schema:
        body["response_format"] = {"type": "json_schema", "json_schema": {"name": "out", "schema": schema}}
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
            "to remember from it, especially any request or advice about what to do. Only what was actually "
            "said: never invent a request. "
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
    # Did the Watcher ask for a specific job? Becomes a request the colonist's brain weighs.
    request = "none"
    try:
        out = llm([{"role": "system", "content": "You classify requests into JSON."},
                   {"role": "user", "content": f"Conversation:\n{convo}\n\nDid the Watcher ask {name} to do one of "
                    f"these jobs: {', '.join(ASKABLE)}? Answer with the job, or none."}],
                  max_tokens=30, temperature=0.0,
                  schema={"type": "object", "properties": {"request": {"enum": ASKABLE + ["none"]}}, "required": ["request"]})
        request = json.loads(out).get("request", "none")
        # Only if the Watcher's own words ask for that job (small models over-extract).
        said = [{"who": "Watcher", "text": m["content"]} for m in req.transcript if m["role"] == "user"]
        if request != "none" and not _asked_in_words("Watcher", request, said, WATCHER_CUES):
            request = "none"
    except Exception:
        pass
    return {"memory": mem, "request": request}


# ---------------------------------------------------------------------------- colonists talking

RES = ["food", "water", "wood", "stone"]
ASKABLE = ["gather_food", "fetch_water", "gather_wood", "gather_stone", "farm", "build", "check_stores"]
LEVEL_VALUE = {"none": 0, "low": 0.25, "some": 0.7, "plenty": 1.6}   # x a typical amount per resource
TYPICAL = {"food": 60, "water": 30, "wood": 40, "stone": 40}
LINE_RE = re.compile(r"^\s*\**\s*([A-Z][\w'\u2019\- ]{0,30}?)\s*\**\s*:\s*(.+)$")


class ConverseRequest(BaseModel):
    a: dict[str, Any]
    b: dict[str, Any]
    intent: str = "chat"          # chat | news | ask
    topic: str = ""               # ask: what A wants B to do (label)
    topic_id: str = ""            # ask: the action id behind the label
    relation: str = ""            # how A sees B


def _speaker_block(c: dict) -> str:
    knows = "; ".join(c.get("beliefs", [])) or "nothing about the stores"
    mems = "; ".join(c.get("memories", [])[:3])
    return (f"{c['name']} ({', '.join(c.get('traits', []))}). Feels: {', '.join(c.get('need_words', []))}. "
            f"Doing: {c.get('action', 'idle')}. Believes the stores hold: {knows}. "
            f"Recent memories: {mems or 'none'}.")


def converse_dialogue(req: ConverseRequest) -> list[dict]:
    a, b = req.a["name"], req.b["name"]
    goal = {"chat": f"They are just chatting. They may mention what they know about the colony's stores if it's on their mind.",
            "news": f"{a} came over to tell {b} what {a} saw in the stores.",
            "ask": f"{a} came over to ask {b} to {req.topic}, because {a} thinks the colony needs it."}[req.intent if req.intent in ("chat", "news", "ask") else "chat"]
    sys = ("You write short, natural conversations between two villagers in a small colony where everyone "
           "shares food, water, wood and stone. Each person only knows what is listed for them; they may be "
           "vague or a bit wrong about amounts. Write 4 to 6 lines, alternating speakers, each line as "
           "'Name: words', under 20 words each, in the first person ('I', 'we'). Only spoken words: no narration, "
           "no actions, no stage directions, and don't invent buildings or events that aren't mentioned.")
    user = (f"{_speaker_block(req.a)}\n{_speaker_block(req.b)}\n{a} sees {b} as {req.relation or 'a neighbour'}.\n"
            f"{goal}\nWrite the conversation, starting with {a}.")
    text = llm([{"role": "system", "content": sys}, {"role": "user", "content": user}], max_tokens=220, temperature=0.8)
    lines = []
    for raw in text.replace("**", "").splitlines():
        m = LINE_RE.match(raw)
        if not m:
            continue
        who, said = m.group(1).strip(), m.group(2).strip().strip('"')
        if who not in (a, b):
            continue
        said = re.sub(r"\*[^*]{1,80}\*", "", said).strip()
        if said:
            lines.append({"who": who, "text": said})
    return lines[:6]


EXTRACT_SCHEMA = {
    "type": "object",
    "properties": {
        "told": {"type": "array", "items": {"type": "object", "properties": {
            "from": {"type": "string"},
            "resource": {"enum": RES},
            "amount": {"type": ["integer", "null"]},
            "level": {"enum": ["none", "low", "some", "plenty", "unclear"]},
        }, "required": ["from", "resource", "amount", "level"]}},
        "requests": {"type": "array", "items": {"type": "object", "properties": {
            "from": {"type": "string"},
            "action": {"enum": ASKABLE},
        }, "required": ["from", "action"]}},
    },
    "required": ["told", "requests"],
}


def converse_extract(req: ConverseRequest, lines: list[dict]) -> dict:
    a, b = req.a["name"], req.b["name"]
    convo = "\n".join(f"{l['who']}: {l['text']}" for l in lines)
    user = (f"Conversation between {a} and {b}:\n{convo}\n\n"
            "List every statement where one of them tells the other how much food, water, wood or stone "
            "the colony's stores have (amount as a number if they said one, and a level), and every time one "
            f"asks the other to do a job ({', '.join(ASKABLE)}). 'from' is the speaker's name. "
            "Only include things actually said. Empty lists if none.")
    out = llm([{"role": "system", "content": "You extract facts from dialogue into JSON."},
               {"role": "user", "content": user}], max_tokens=300, temperature=0.0, schema=EXTRACT_SCHEMA)
    return json.loads(out)


NUM_WORDS = {w: i for i, w in enumerate("zero one two three four five six seven eight nine ten eleven twelve "
                                         "thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty".split())}
NUM_WORDS.update({"thirty": 30, "forty": 40, "fifty": 50, "sixty": 60, "seventy": 70, "eighty": 80,
                  "ninety": 90, "hundred": 100, "a hundred": 100})


ASK_CUES = re.compile(r"\b(could you|can you|would you|will you|please|help me|help us|we need you|you should|"
                      r"why don't you|let's|go and|need someone)\b", re.I)
# The Watcher gives instructions, so plain imperatives ("go chop wood") count as asking too. Only
# used for the Watcher's own words, never colonist-to-colonist talk ("I'll go get water" isn't a request).
WATCHER_CUES = re.compile(ASK_CUES.pattern[:-4] + r"|go|get|grab|bring|fetch|chop|cut|gather|pick|mine|build|"
                          r"farm|plant|harvest|warm up|rest|take a break|count|check)\b", re.I)
JOB_WORDS = {
    "gather_food": r"\b(berr\w*|food|pick\w*|fruit)",
    "fetch_water": r"\bwater\b|\bbucket|\b(the|a|from the|to the) well\b|\briver",
    "gather_wood": r"\b(wood|chop\w*|logs?|trees?|timber|firewood)\b(?! smoke)",
    "gather_stone": r"\b(stones?|rocks?|mine|mining|quarry)\b",
    "farm": r"\b(farm\w*|plant\w*|harvest\w*|crops?|fields?|sow\w*)\b",
    "build": r"\b(build\w*|construct\w*|house|finish the)\b",
    "check_stores": r"\b(check|count|look at|see what)\b.*\b(stores?|stock\w*|supplies|storehouse)\b|\bhow much\b.*\b(left|have|got)\b",
    "warm_up": r"\b(warm (yourself |you )?up|by the fire|get warm|campfire)\b",
    "rest": r"\b(rest|take a break|sit down|have a break|catch your breath)\b",
    "hunt": r"\b(hunt\w*|deer|rabbits?|spear|meat|venison)\b",
    "explore": r"\b(explor\w*|scout\w*|look around|search (for|the)|find (some|more|new)|go and see what|see what'?s (out|beyond|over)|wander\w*|map the land|what you find)\b",
}
# Clear signs a message is NOT asking for a job now: negations, questions about the past, thanks.
NOT_ASKING = re.compile(r"\b(don'?t|do not|never mind|nevermind|no need|stop|quit|forget about|leave the|"
                        r"thanks?|thank you|yesterday|earlier|last night|did you|have you|were you|"
                        r"i love|i like)\b", re.I)


def _asked_in_words(name: str, action: str, lines: list[dict], cues: re.Pattern = ASK_CUES) -> bool:
    for l in lines:
        if l["who"] == name and cues.search(l["text"]) and re.search(JOB_WORDS[action], l["text"], re.I):
            return True
    return False


def _belief_values(c: dict) -> dict:
    """{'food': 12, ...} from belief phrases like 'food ~12 (seen 3h ago)'; unknown ones are left out."""
    out = {}
    for b in c.get("beliefs", []):
        m = re.match(r"(\w+) ~(\d+)", b)
        if m and m.group(1) in RES:
            out[m.group(1)] = int(m.group(2))
    return out


def _numbers_said(name: str, lines: list[dict]) -> set:
    nums = set()
    for l in lines:
        if l["who"] != name:
            continue
        t = l["text"].lower()
        nums.update(int(x) for x in re.findall(r"\d+", t))
        for w, v in NUM_WORDS.items():
            if re.search(rf"\b{w}\b", t):
                nums.add(v)
    return nums


@app.post("/converse")
def converse(req: ConverseRequest) -> dict:
    t = time.time()
    try:
        lines = converse_dialogue(req)
        if not lines:
            return {"lines": [], "facts": [], "requests": [], "error": "no dialogue"}
        ex = converse_extract(req, lines)
    except Exception as e:
        return {"lines": [], "facts": [], "requests": [], "error": repr(e)}
    names = {req.a["name"]: req.b["name"], req.b["name"]: req.a["name"]}
    beliefs = {req.a["name"]: _belief_values(req.a), req.b["name"]: _belief_values(req.b)}
    said = {n: _numbers_said(n, lines) for n in names}
    facts = []
    for f in ex.get("told", []):
        sp, res = f.get("from"), f.get("resource")
        if sp not in names or res not in RES:
            continue
        known = beliefs[sp].get(res)
        if known is None:
            continue          # they have no idea about it, so they can't have told it (extraction slip)
        amt = f.get("amount")
        if amt is not None and int(amt) not in said[sp]:
            amt = known       # number not actually in their words: they meant what they believe
        if amt is None:
            lvl = f.get("level", "unclear")
            if lvl not in LEVEL_VALUE:
                continue
            amt = round(LEVEL_VALUE[lvl] * TYPICAL[res])
        facts.append({"from": sp, "to": names[sp], "key": res, "value": max(0, int(amt))})
    # Requests must really be in the words: the speaker asks ("could you", "please", ...) and names
    # the job. Small models over-extract, so this keeps only grounded ones (one per speaker).
    reqs, asked = [], set()
    for r in ex.get("requests", []):
        sp, act = r.get("from"), r.get("action")
        if sp in names and act in ASKABLE and sp not in asked and _asked_in_words(sp, act, lines):
            reqs.append({"from": sp, "to": names[sp], "action": act})
            asked.add(sp)
    if req.intent == "ask" and req.topic_id and req.a["name"] not in asked:
        reqs.append({"from": req.a["name"], "to": req.b["name"], "action": req.topic_id})   # why they came
    return {"lines": lines, "facts": facts, "requests": reqs, "llm": True, "latency_ms": (time.time() - t) * 1000.0}


class MeetingRequest(BaseModel):
    people: list[dict[str, Any]]
    options: list[dict[str, Any]]     # [{id, label}] what the meeting could agree to do
    seats: int = 0
    overflow: int = 0
    place: str = "meeting hall"
    plan_items: int = 3
    built: list[str] = []            # what the colony has, so the dialogue doesn't invent places


MEETING_STOP = {"some", "more", "with", "the", "new", "a", "an"}   # every other word of the job must be said (verb too)


def _grounded(label: str, said: str, wished: set) -> bool:
    """A plan item counts if someone came wanting it, or its distinctive words were all said."""
    if label in wished:
        return True
    words = [w for w in re.findall(r"[a-z]+", label.lower()) if len(w) > 2 and w not in MEETING_STOP]
    return bool(words) and all(w in said for w in words)


@app.post("/meeting")
def meeting(req: MeetingRequest) -> dict:
    """The weekly meeting: a short discussion among the people there (each knows only their own
    beliefs and has a couple of things on their mind), then the plan they agreed, picked from the
    options and grounded in what was said."""
    t = time.time()
    names = [p["name"] for p in req.people]
    speakers = sorted(req.people, key=lambda p: -len(p.get("wishes", [])))[:6]
    blocks = "\n".join(f"{_speaker_block(p)} On their mind: {', '.join(p.get('wishes', [])) or 'nothing pressing'}."
                       for p in speakers)
    crowd = (f" The {req.place} seats {req.seats}; {req.overflow} people had to stand outside." if req.overflow > 0 else "")
    built = ", ".join(req.built) or "a campfire"
    sys = ("You write the weekly meeting of a small colony where everyone shares food, water, wood and stone. "
           "Each person only knows what is listed for them and may be a bit wrong about amounts. Write 5 to 8 lines, "
           "each as 'Name: words', under 22 words. People talk like neighbours, in the first person ('I', 'we'): they "
           "say what worries them and why, in their own words; they don't recite lists of numbers or feelings. "
           "They react to each other and end by agreeing who will do what. Only spoken words: no narration or "
           f"actions. The colony has only: {built}. Never mention any other place or building.")
    user = (f"The meeting is at the {req.place}. {len(names)} people came: {', '.join(names)}.{crowd}\n{blocks}\n"
            f"Write the meeting.")
    try:
        text = llm([{"role": "system", "content": sys}, {"role": "user", "content": user}], max_tokens=320, temperature=0.8)
    except Exception as e:
        return {"lines": [], "items": [], "error": repr(e)}
    lines = []
    for raw in text.replace("**", "").splitlines():
        m = LINE_RE.match(raw)
        if m and m.group(1).strip() in names:
            said = re.sub(r"\*[^*]{1,80}\*", "", m.group(2).strip().strip('"')).strip()
            if said:
                lines.append({"who": m.group(1).strip(), "text": said})
    lines = lines[:8]
    if not lines:
        return {"lines": [], "items": [], "error": "no dialogue"}
    ids = [o["id"] for o in req.options]
    labels = {o["id"]: o["label"] for o in req.options}
    schema = {"type": "object", "properties": {"plan": {"type": "array", "maxItems": req.plan_items, "items": {
        "type": "object", "properties": {"id": {"enum": ids}, "who": {"type": "array", "items": {"enum": names}}},
        "required": ["id", "who"]}}}, "required": ["plan"]}
    convo = "\n".join(f"{l['who']}: {l['text']}" for l in lines)
    opts = "\n".join(f"- {o['id']}: {o['label']}" for o in req.options)
    try:
        out = json.loads(llm([{"role": "system", "content": "You record what a meeting agreed, as JSON."},
                              {"role": "user", "content": f"Meeting:\n{convo}\n\nOptions:\n{opts}\n\nWhich options did they "
                               "agree the colony should do (at most %d), and who said they would do each? Only what was "
                               "actually said." % req.plan_items}], max_tokens=200, temperature=0.1, schema=schema))
    except Exception:
        out = {"plan": []}
    # Grounding: keep an item only if someone wanted it or its words came up in the meeting.
    said = convo.lower()
    wished = {w for p in req.people for w in p.get("wishes", [])}
    items = []
    for it in out.get("plan", []):
        oid = it.get("id")
        if oid not in labels or any(x["id"] == oid for x in items):
            continue
        if _grounded(labels[oid], said, wished):
            items.append({"id": oid, "who": [n for n in it.get("who", []) if n in names]})
    return {"lines": lines, "items": items, "llm": True, "latency_ms": (time.time() - t) * 1000.0}


class IntentRequest(BaseModel):
    colonist: str = ""
    message: str


@app.post("/intent")
def intent(req: IntentRequest) -> dict:
    """Is the Watcher asking for a job right now, in this one message? A job has to be named in the
    words (fast check). The model then judges whether it's really being asked for now, rather than
    refused, asked about, thanked for or just mentioned. Obvious negations skip the model."""
    text = req.message.strip()
    hits = [job for job, pat in JOB_WORDS.items() if re.search(pat, text, re.I)]
    if not hits:
        return {"request": "none", "why": "no job named"}
    neg = NOT_ASKING.search(text)
    cue = WATCHER_CUES.search(text)
    if neg and not cue:
        return {"request": "none", "why": "not asking"}
    if text.endswith("?") and not ASK_CUES.search(text):
        return {"request": "none", "why": "a question, not a request"}   # "what about the river?"
    # Narrow to the jobs named in the instruction itself ("we need food, go work the farm" -> farm).
    parts = [p for p in re.split(r",|;|\.|\bbut\b|\band\b|\bthen\b", text, flags=re.I) if p.strip()]
    told = [j for j in hits if any(WATCHER_CUES.search(p) and not NOT_ASKING.search(p)
                                   and re.search(JOB_WORDS[j], p, re.I) for p in parts)]
    if told:
        hits = told
    try:
        out = llm([{"role": "system", "content": "You decide whether a message gives someone a job to do now. Answer in JSON."},
                   {"role": "user", "content":
                    f"The Watcher says to {req.colonist or 'a villager'}: \"{text}\"\n\n"
                    "Is the Watcher telling or asking them to do one of these jobs NOW? "
                    f"Jobs: {', '.join(hits)}.\n"
                    "Answer none if the job is refused or cancelled (\"don't\", \"no need\", \"never mind\"), "
                    "only asked about (\"did you...?\"), thanked for, or just mentioned in passing.\n"
                    "Examples: \"go chop some wood\" -> gather_wood. \"can you fetch water\" -> fetch_water. "
                    "\"don't chop wood\" -> none. \"did you fetch water?\" -> none. "
                    "\"thanks for the wood\" -> none. \"stop farming and get water\" -> fetch_water. "
                    "When a reason and a job are both mentioned, pick the job they are told to do: "
                    "\"we're hungry, go harvest the fields\" -> farm."}],
                  max_tokens=30, temperature=0.0,
                  schema={"type": "object", "properties": {"request": {"enum": hits + ["none"]}}, "required": ["request"]})
        r = json.loads(out).get("request", "none")
    except Exception:
        r = hits[0] if (cue and not neg) else "none"   # model unreachable: only clear instructions count
    if r != "none" and neg:
        # Keep it only if the job is named in a part of the message that isn't negated
        # ("stop farming and get water" asks for water; "please don't fetch water" asks for nothing).
        if not any(re.search(JOB_WORDS[r], p, re.I) and not NOT_ASKING.search(p) for p in parts):
            r = "none"
    return {"request": r}


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
