"""Train Colony's text model on a Colab GPU (ARCHITECTURE.md section 9). The cheapest GPU (a T4)
is plenty: about 15 minutes for 5,000 decisions.

The model reads a decision the way Claude Haiku does, as words: the situation, then the
numbered options. It encodes the situation once and each option line once with a small encoder
(BAAI/bge-small-en-v1.5, 33M parameters), and a small head scores every (situation, option) pair.
It learns the teacher's choice (Haiku's, from `colony-run label`) by softmax over each decision's
options. Option lines repeat a lot ("Forage at the berry bush to the north-east (about 192 min)"),
so the game can cache their encodings: a decision costs one encoder pass on the situation plus the
new options, a few milliseconds on the CPU.

Run it from the notebook (text_model.ipynb), or in a cell:

    !pip -q install onnx onnxruntime
    %run train_colab.py

(Colab already has torch and transformers.)

Reads  DATA (default: the dataset pushed to the repo, brain/data/haiku-v4/text.jsonl.gz)
Writes OUT/ (default /content/text_model_v2):
       encoder.onnx (+ encoder.int8.onnx), head.onnx, tokenizer.json, report.json
Env overrides: DATA, OUT, BASE, EPOCHS, MAX_LEN, LR.
"""

import gzip
import io
import json
import math
import os
import random
import re
import time
import urllib.request

# The base model is public: don't look for a Hugging Face token (on Colab that asks for access to
# your saved secrets and waits until someone answers).
os.environ.setdefault("HF_HUB_DISABLE_IMPLICIT_TOKEN", "1")

import torch
import torch.nn as nn
import torch.nn.functional as F
from transformers import AutoModel, AutoTokenizer

DATA = os.environ.get("DATA", "https://raw.githubusercontent.com/DeclanJavers/colony/main/brain/data/haiku-v4/text.jsonl.gz")
OUT = os.environ.get("OUT", "/content/text_model_v2")
BASE = os.environ.get("BASE", "BAAI/bge-small-en-v1.5")
EPOCHS = int(os.environ.get("EPOCHS", "4"))
MAX_LEN = int(os.environ.get("MAX_LEN", "256"))
OPT_LEN = 48
LR = float(os.environ.get("LR", "3e-5"))
DECISIONS_PER_STEP = 8
SEED = 0

assert torch.cuda.is_available(), "No GPU: Runtime > Change runtime type > T4 GPU"
dev = "cuda"
print("GPU:", torch.cuda.get_device_name(0))
random.seed(SEED)
torch.manual_seed(SEED)
os.makedirs(OUT, exist_ok=True)

# ------------------------------------------------------------------------------------- the data

OPTION = re.compile(r"^(\d+)\. (.*)$")


def split(text):
    """The situation, and the option lines in order (the numbers the teacher answered with)."""
    head, _, tail = text.partition("What do you do next?")
    options = [m.group(2).strip() for m in (OPTION.match(l.strip()) for l in tail.splitlines()) if m]
    return head.strip(), options


def read(path):
    if path.startswith("http"):
        raw = urllib.request.urlopen(path).read()
    else:
        raw = open(path, "rb").read()
    if path.endswith(".gz"):
        raw = gzip.decompress(raw)
    rows = []
    for line in io.StringIO(raw.decode()):
        r = json.loads(line)
        situation, options = split(r["prompt"])
        if len(options) < 2 or not (1 <= r["choice"] <= len(options)):
            continue
        rows.append({"split": r["split"], "situation": situation, "options": options, "choice": r["choice"] - 1, "rules": (r.get("rules_choice") or 0) - 1})
    return rows


rows = read(DATA)
train = [r for r in rows if r["split"] == "train"]
held = [r for r in rows if r["split"] != "train"]
print(f"{len(rows)} decisions: {len(train)} to train on, {len(held)} held out")

# ------------------------------------------------------------------------------------ the model

tok = AutoTokenizer.from_pretrained(BASE)


class TextModel(nn.Module):
    def __init__(self):
        super().__init__()
        self.enc = AutoModel.from_pretrained(BASE)
        d = self.enc.config.hidden_size
        self.head = nn.Sequential(nn.Linear(4 * d, 512), nn.GELU(), nn.Linear(512, 1))

    def encode(self, ids, mask):
        # The [CLS] vector, as bge-small is trained to use.
        return self.enc(input_ids=ids, attention_mask=mask).last_hidden_state[:, 0]

    def score(self, s, o):
        """s: [d] (one situation), o: [n, d] (its options) -> [n] scores."""
        s = s.expand_as(o)
        return self.head(torch.cat([s, o, s * o, (s - o).abs()], dim=-1)).squeeze(-1)


model = TextModel().to(dev)


def batch_tokens(texts, max_len):
    t = tok(texts, padding=True, truncation=True, max_length=max_len, return_tensors="pt")
    return t["input_ids"].to(dev), t["attention_mask"].to(dev)


def decision_scores(rs):
    """Scores for a few decisions at once: situations in one pass, all their options in another."""
    s = model.encode(*batch_tokens([r["situation"] for r in rs], MAX_LEN))
    opts = [o for r in rs for o in r["options"]]
    o = model.encode(*batch_tokens(opts, OPT_LEN))
    out, k = [], 0
    for i, r in enumerate(rs):
        n = len(r["options"])
        out.append(model.score(s[i], o[k : k + n]))
        k += n
    return out


@torch.no_grad()
def agreement(rs):
    model.eval()
    hit = rules_hit = 0
    for i in range(0, len(rs), 16):
        chunk = rs[i : i + 16]
        with torch.autocast("cuda", dtype=torch.float16):
            scores = decision_scores(chunk)
        for r, sc in zip(chunk, scores):
            pick = int(sc.argmax())
            hit += pick == r["choice"]
            rules_hit += pick == r["rules"]
    model.train()
    return hit / max(len(rs), 1), rules_hit / max(len(rs), 1)


# ---------------------------------------------------------------------------------- training

head_params = list(model.head.parameters())
opt = torch.optim.AdamW([{"params": model.enc.parameters(), "lr": LR}, {"params": head_params, "lr": 1e-3}], weight_decay=0.01)
steps = EPOCHS * math.ceil(len(train) / DECISIONS_PER_STEP)
sched = torch.optim.lr_scheduler.LambdaLR(opt, lambda k: min(1.0, (k + 1) / 100) * max(0.0, 1 - k / steps))
scaler = torch.amp.GradScaler("cuda")
before = agreement(held)
print(f"before training: held-out agreement with Haiku {before[0]:.1%}")
t0 = time.time()
history = []
for epoch in range(EPOCHS):
    random.shuffle(train)
    total = 0.0
    for i in range(0, len(train), DECISIONS_PER_STEP):
        chunk = train[i : i + DECISIONS_PER_STEP]
        with torch.autocast("cuda", dtype=torch.float16):
            scores = decision_scores(chunk)
            loss = sum(F.cross_entropy(sc.float().unsqueeze(0), torch.tensor([r["choice"]], device=dev)) for sc, r in zip(scores, chunk)) / len(chunk)
        opt.zero_grad()
        scaler.scale(loss).backward()
        scaler.unscale_(opt)
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        scaler.step(opt)
        scaler.update()
        sched.step()
        total += float(loss)
    agree = agreement(held)
    history.append({"epoch": epoch + 1, "loss": total / math.ceil(len(train) / DECISIONS_PER_STEP), "held_out_agreement": agree[0], "agrees_with_rules": agree[1]})
    print(f"epoch {epoch + 1}: loss {history[-1]['loss']:.3f}, held-out agreement with Haiku {agree[0]:.1%} (with rules {agree[1]:.1%}), {time.time() - t0:.0f} s")

# ------------------------------------------------------------------------------------- export

model.eval().float().cpu()


class Encoder(nn.Module):
    def __init__(self, m):
        super().__init__()
        self.m = m

    def forward(self, input_ids, attention_mask):
        return self.m.encode(input_ids, attention_mask)


class Head(nn.Module):
    def __init__(self, m):
        super().__init__()
        self.m = m

    def forward(self, situation, options):
        """situation: [1, d], options: [n, d] -> [n]."""
        return self.m.score(situation[0], options)


ids = torch.ones(1, 16, dtype=torch.long)
mask = torch.ones(1, 16, dtype=torch.long)
torch.onnx.export(Encoder(model), (ids, mask), f"{OUT}/encoder.onnx", input_names=["input_ids", "attention_mask"], output_names=["vector"],
                  dynamic_axes={"input_ids": {0: "batch", 1: "tokens"}, "attention_mask": {0: "batch", 1: "tokens"}, "vector": {0: "batch"}}, opset_version=17, dynamo=False)
d = model.enc.config.hidden_size
torch.onnx.export(Head(model), (torch.zeros(1, d), torch.zeros(3, d)), f"{OUT}/head.onnx", input_names=["situation", "options"], output_names=["scores"],
                  dynamic_axes={"options": {0: "n"}, "scores": {0: "n"}}, opset_version=17, dynamo=False)
tok.backend_tokenizer.save(f"{OUT}/tokenizer.json")

from onnxruntime.quantization import QuantType, quantize_dynamic  # noqa: E402

quantize_dynamic(f"{OUT}/encoder.onnx", f"{OUT}/encoder.int8.onnx", weight_type=QuantType.QInt8)

# How fast it decides on a CPU, through ONNX Runtime (the way the game will run it).
import numpy as np  # noqa: E402
import onnxruntime as ort  # noqa: E402

so = ort.SessionOptions()
so.intra_op_num_threads = 2
enc = ort.InferenceSession(f"{OUT}/encoder.int8.onnx", so, providers=["CPUExecutionProvider"])
head = ort.InferenceSession(f"{OUT}/head.onnx", so, providers=["CPUExecutionProvider"])
sample = held[:50] or train[:50]
t = time.time()
cpu_hit = 0
for r in sample:
    s_ids = tok([r["situation"]], truncation=True, max_length=MAX_LEN, return_tensors="np")
    s = enc.run(None, {"input_ids": s_ids["input_ids"].astype(np.int64), "attention_mask": s_ids["attention_mask"].astype(np.int64)})[0]
    o_ids = tok(r["options"], padding=True, truncation=True, max_length=OPT_LEN, return_tensors="np")
    o = enc.run(None, {"input_ids": o_ids["input_ids"].astype(np.int64), "attention_mask": o_ids["attention_mask"].astype(np.int64)})[0]
    sc = head.run(None, {"situation": s, "options": o})[0]
    cpu_hit += int(np.argmax(sc)) == r["choice"]
ms = (time.time() - t) * 1000 / len(sample)
report = {
    "base": BASE,
    "data": DATA,
    "decisions": len(rows),
    "train": len(train),
    "held_out": len(held),
    "before_training_agreement": before[0],
    "epochs": history,
    "cpu_ms_per_decision_uncached": ms,
    "int8_held_out_agreement_on_50": cpu_hit / len(sample),
    "minutes": (time.time() - t0) / 60,
}
json.dump(report, open(f"{OUT}/report.json", "w"), indent=2)
print(json.dumps(report, indent=2))
print(f"wrote {OUT}: encoder.onnx, encoder.int8.onnx, head.onnx, tokenizer.json, report.json")
