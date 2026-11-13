"""Fine-tune LFM3-1.7B with LoRA on Claude Haiku's answers, on a Colab T4 (M11, ARCHITECTURE.md
section 17). About an hour.

The data is chat examples for every language job the game has (goal and step decisions, talk,
memories in people's own words, reading the player's words): the system and user prompts that
brains::lm makes, and Haiku's JSON answer. Training sees exactly the prompt llama-server will send
in play (the model's own chat template with thinking off), and learns only the answer.

Only a LoRA adapter is trained and kept: the game loads it on top of the base GGUF
(`llama-server --lora`). Before training, a fresh adapter is converted to GGUF with llama.cpp's
converter, so a conversion problem shows up in minutes, not after the hour.

Run it from the notebook (lm_lora.ipynb), or in a cell:

    !pip -q install -U transformers peft accelerate
    %run train_lora.py

Reads  DATA (default: brain/data/lm-v1/lm.jsonl.gz in the repo)
Writes OUT/ (default /content/lm_lora_v1): adapter/ (PEFT), colony-lora.gguf, report.json
Env overrides: DATA, OUT, BASE, EPOCHS, LR, RANK, MAX_LEN.
"""

import gzip
import io
import json
import math
import os
import random
import subprocess
import sys
import time
import urllib.request

# The base model is public: don't look for a Hugging Face token (on Colab that asks for access to
# your saved secrets and waits until someone answers).
os.environ.setdefault("HF_HUB_DISABLE_IMPLICIT_TOKEN", "1")

import torch
from peft import LoraConfig, get_peft_model
from transformers import AutoModelForCausalLM, AutoTokenizer

DATA = os.environ.get("DATA", "https://raw.githubusercontent.com/DeclanJavers/colony/main/brain/data/lm-v1/lm.jsonl.gz")
OUT = os.environ.get("OUT", "/content/lm_lora_v1")
BASE = os.environ.get("BASE", "LiquidAI/LFM3-1.7B")
EPOCHS = float(os.environ.get("EPOCHS", "1"))
LR = float(os.environ.get("LR", "2e-4"))
RANK = int(os.environ.get("RANK", "16"))
MAX_LEN = int(os.environ.get("MAX_LEN", "1600"))
TOKENS_PER_STEP = 12_000  # examples are packed into steps of about this many tokens
SEED = 0
TARGETS = ["q_proj", "k_proj", "v_proj", "o_proj", "w1", "w2", "w3", "in_proj", "out_proj"]

assert torch.cuda.is_available(), "No GPU: Runtime > Change runtime type > T4 GPU"
dev = "cuda"
print("GPU:", torch.cuda.get_device_name(0))
random.seed(SEED)
torch.manual_seed(SEED)
os.makedirs(OUT, exist_ok=True)
t_start = time.time()

# ------------------------------------------------------------------------------------- the data


def read(path):
    raw = urllib.request.urlopen(path).read() if path.startswith("http") else open(path, "rb").read()
    if path.endswith(".gz"):
        raw = gzip.decompress(raw)
    return [json.loads(l) for l in io.StringIO(raw.decode()) if l.strip()]


rows = read(DATA)
train = [r for r in rows if r["split"] == "train"]
held = [r for r in rows if r["split"] != "train"]
by_task = {}
for r in train:
    by_task[r["task"]] = by_task.get(r["task"], 0) + 1
print(f"{len(rows)} examples: {len(train)} to train on {by_task}, {len(held)} held out")

tok = AutoTokenizer.from_pretrained(BASE, trust_remote_code=True)


def prompt_text(r):
    """Exactly what llama-server sends: the chat template, thinking off, ready for the answer."""
    msgs = [{"role": "system", "content": r["system"]}, {"role": "user", "content": r["user"]}]
    return tok.apply_chat_template(msgs, tokenize=False, add_generation_prompt=True, enable_thinking=False)


END = "<|im_end|>"


def encode(r):
    p = tok(prompt_text(r), add_special_tokens=False)["input_ids"]
    a = tok(r["assistant"] + END, add_special_tokens=False)["input_ids"]
    if len(p) + len(a) > MAX_LEN:
        p = p[-(MAX_LEN - len(a)):]  # keep the end of the situation and the question
    return p + a, [-100] * len(p) + a


t0 = time.time()
enc = [encode(r) for r in train]
lengths = sorted(len(e[0]) for e in enc)
print(f"tokenised in {time.time() - t0:.0f} s: median {lengths[len(lengths) // 2]} tokens, longest {lengths[-1]}")

# ------------------------------------------------------------------------------------ the model

model = AutoModelForCausalLM.from_pretrained(BASE, trust_remote_code=True, torch_dtype=torch.float16).to(dev)
model.config.use_cache = False
model.gradient_checkpointing_enable(gradient_checkpointing_kwargs={"use_reentrant": False})
model.enable_input_require_grads()
present = {n.split(".")[-1] for n, m in model.named_modules() if isinstance(m, torch.nn.Linear)}
targets = [t for t in TARGETS if t in present]
print("LoRA on:", targets)
model = get_peft_model(model, LoraConfig(r=RANK, lora_alpha=2 * RANK, lora_dropout=0.05, target_modules=targets, task_type="CAUSAL_LM"))
# The adapter trains in fp32 (fp16 weights underneath).
for n, p in model.named_parameters():
    if p.requires_grad:
        p.data = p.data.float()
model.print_trainable_parameters()

# ----------------------------------------------------------- can llama.cpp read the adapter?

LLAMA = "/content/llama.cpp"


def convert(adapter_dir, out_file):
    if not os.path.exists(LLAMA):
        subprocess.run(["git", "clone", "--depth", "1", "https://github.com/ggml-org/llama.cpp", LLAMA], check=True)
        subprocess.run([sys.executable, "-m", "pip", "-q", "install", "-e", f"{LLAMA}/gguf-py"], check=True)
    from huggingface_hub import snapshot_download

    base_dir = snapshot_download(BASE)
    r = subprocess.run([sys.executable, f"{LLAMA}/convert_lora_to_gguf.py", adapter_dir, "--base", base_dir, "--outfile", out_file, "--outtype", "f16"], capture_output=True, text=True)
    print(r.stdout[-1500:], r.stderr[-3000:])
    return r.returncode == 0 and os.path.exists(out_file)


model.save_pretrained(f"{OUT}/adapter-untrained")
if not convert(f"{OUT}/adapter-untrained", f"{OUT}/untrained.gguf"):
    raise SystemExit("llama.cpp couldn't convert the adapter (see above): stopping before training")
print("conversion works:", os.path.getsize(f"{OUT}/untrained.gguf") // 1_000_000, "MB")

# ---------------------------------------------------------------------------------- training


def batches(order):
    """Examples of similar length together, up to about TOKENS_PER_STEP tokens a step."""
    order = sorted(order, key=lambda i: len(enc[i][0]))
    groups, cur, width = [], [], 0
    for i in order:
        w = max(width, len(enc[i][0]))
        if cur and w * (len(cur) + 1) > TOKENS_PER_STEP:
            groups.append(cur)
            cur, w = [], len(enc[i][0])
        cur.append(i)
        width = w
    if cur:
        groups.append(cur)
    random.shuffle(groups)
    return groups


def collate(ids):
    w = max(len(enc[i][0]) for i in ids)
    x = torch.full((len(ids), w), tok.pad_token_id or 0, dtype=torch.long)
    y = torch.full((len(ids), w), -100, dtype=torch.long)
    m = torch.zeros((len(ids), w), dtype=torch.long)
    for k, i in enumerate(ids):
        a, b = enc[i]
        x[k, : len(a)] = torch.tensor(a)
        y[k, : len(b)] = torch.tensor(b)
        m[k, : len(a)] = 1
    return x.to(dev), y.to(dev), m.to(dev)


inner = model.base_model.model  # the Lfm3ForCausalLM under the adapter


def answer_loss(x, y, m):
    """Next-token loss on the answer only: the vocabulary's logits are worked out just where
    there's an answer token to predict (a 128k vocabulary over every prompt token won't fit a T4)."""
    h = inner.model(input_ids=x, attention_mask=m).last_hidden_state[:, :-1]
    target = y[:, 1:]
    keep = target != -100
    logits = inner.lm_head(h[keep])
    return torch.nn.functional.cross_entropy(logits.float(), target[keep])


params = [p for p in model.parameters() if p.requires_grad]
opt = torch.optim.AdamW(params, lr=LR, weight_decay=0.0)
plan = []
for e in range(math.ceil(EPOCHS)):
    g = batches(list(range(len(enc))))
    plan += g[: int(len(g) * min(1.0, EPOCHS - e))]
steps = len(plan)
sched = torch.optim.lr_scheduler.LambdaLR(opt, lambda k: min(1.0, (k + 1) / 30) * max(0.05, 0.5 * (1 + math.cos(math.pi * k / steps))))
scaler = torch.amp.GradScaler("cuda")
model.train()
history, t0, run = [], time.time(), 0.0
for k, ids in enumerate(plan):
    x, y, m = collate(ids)
    with torch.autocast("cuda", dtype=torch.float16):
        loss = answer_loss(x, y, m)
    opt.zero_grad()
    scaler.scale(loss).backward()
    scaler.unscale_(opt)
    torch.nn.utils.clip_grad_norm_(params, 1.0)
    scaler.step(opt)
    scaler.update()
    sched.step()
    run = 0.95 * run + 0.05 * float(loss) if k else float(loss)
    if k % 20 == 0 or k == steps - 1:
        el = time.time() - t0
        print(f"step {k + 1}/{steps}: loss {run:.3f}, {el / 60:.1f} min, about {el / (k + 1) * (steps - k - 1) / 60:.0f} min left", flush=True)
        history.append({"step": k + 1, "loss": run})

# ------------------------------------------------------------------- a look at held-out jobs

model.eval()
model.config.use_cache = True


@torch.no_grad()
def answer(r, max_new=200):
    ids = tok(prompt_text(r), return_tensors="pt", add_special_tokens=False).to(dev)
    with torch.autocast("cuda", dtype=torch.float16):
        out = model.generate(**ids, max_new_tokens=max_new, do_sample=False, eos_token_id=tok.convert_tokens_to_ids(END))
    return tok.decode(out[0, ids["input_ids"].shape[1]:], skip_special_tokens=True).strip()


def first_json(s):
    try:
        return json.loads(s)
    except ValueError:
        a, b = s.find("{"), s.rfind("}")
        try:
            return json.loads(s[a : b + 1]) if a >= 0 and b > a else None
        except ValueError:
            return None


look = {}
for task in sorted({r["task"] for r in held}):
    rs = [r for r in held if r["task"] == task][:12]
    valid = agree = 0
    samples = []
    for r in rs:
        got = answer(r)
        j = first_json(got)
        valid += j is not None
        if task in ("goal", "step") and j is not None:
            agree += j.get("choice") == first_json(r["assistant"]).get("choice")
        if len(samples) < 2:
            samples.append({"haiku": r["assistant"], "tuned": got})
    look[task] = {"n": len(rs), "valid_json": valid, "same_choice_as_haiku": agree if task in ("goal", "step") else None, "samples": samples}
    print(task, json.dumps(look[task], indent=1)[:1200])

# ---------------------------------------------------------------------------------- export

model.save_pretrained(f"{OUT}/adapter")
ok = convert(f"{OUT}/adapter", f"{OUT}/colony-lora.gguf")
report = {
    "base": BASE,
    "data": DATA,
    "examples": len(rows),
    "train": by_task,
    "held_out": len(held),
    "targets": targets,
    "rank": RANK,
    "epochs": EPOCHS,
    "steps": steps,
    "loss": history,
    "held_out_look": look,
    "gguf": ok,
    "gguf_mb": os.path.getsize(f"{OUT}/colony-lora.gguf") / 1e6 if ok else None,
    "minutes": (time.time() - t_start) / 60,
}
json.dump(report, open(f"{OUT}/report.json", "w"), indent=2)
print(json.dumps({k: v for k, v in report.items() if k != "held_out_look"}, indent=2))
print(f"wrote {OUT}: adapter/, colony-lora.gguf, report.json")
