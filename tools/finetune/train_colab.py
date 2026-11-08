"""Fine-tune Laya on a colony dataset, on a Colab GPU. Use the cheapest GPU (T4) unless there's a
reason not to: v1 ran on an A100 and took ~40 min, which was overkill.
Run from a notebook cell after mounting Drive:

    !pip -q install laya==0.4.29
    %run /content/drive/MyDrive/colony_laya/train_colab.py

Reads  MyDrive/colony_laya/<DATA>/{train,eval}.jsonl.gz   (made by tools/finetune/make_data.py)
Writes MyDrive/colony_laya/<OUT>/                         (a checkpoint laya.load() can read)
Env overrides: DATA, OUT, EPOCHS, MAX_TRAIN, MAX_EVAL.
"""

import gzip
import json
import os
import random
import shutil
import time

import torch
from laya.train import TrainConfig, finetune, resolve_checkpoint_dir

DRIVE = "/content/drive/MyDrive/colony_laya"
DATA = os.environ.get("DATA", "data_v1")
OUT = os.environ.get("OUT", "laya_colony_v1")
EPOCHS = int(os.environ.get("EPOCHS", "2"))
MAX_TRAIN = int(os.environ.get("MAX_TRAIN", "24000"))
MAX_EVAL = int(os.environ.get("MAX_EVAL", "1500"))
WORK = "/content/work"

assert torch.cuda.is_available(), "No GPU: Runtime > Change runtime type > GPU"
gpu = torch.cuda.get_device_name(0)
bf16 = torch.cuda.is_bf16_supported()
print(f"GPU: {gpu}  bf16: {bf16}")

os.makedirs(WORK, exist_ok=True)


def unpack(name: str, limit: int) -> str:
    rows = gzip.open(f"{DRIVE}/{DATA}/{name}.jsonl.gz", "rt").read().splitlines()
    random.Random(0).shuffle(rows)
    rows = rows[:limit] if limit else rows
    path = f"{WORK}/{name}.jsonl"
    with open(path, "w") as f:
        for line in rows:
            r = json.loads(line)
            r.pop("meta", None)
            f.write(json.dumps(r) + "\n")
    print(f"{name}: {len(rows)} rows")
    return path


train_path = unpack("train", MAX_TRAIN)
eval_path = unpack("eval", MAX_EVAL)

base = resolve_checkpoint_dir("convaiinnovations/laya")
print("base checkpoint:", base)

# A bigger GPU takes bigger micro-batches; the effective batch stays 64.
big = any(k in gpu for k in ("A100", "H100", "L4"))
cfg = TrainConfig(
    epochs=EPOCHS,
    micro_batch=16 if big else 8,
    grad_accum=4 if big else 8,
    loss="soft-ce",                 # distilling soft targets; laya's notes measured no gain from rlcd here
    shuffle_options=("choice",),    # option order changes per colonist, so don't learn positions
    eval_data=eval_path,
    log_every=50,
)
t = time.time()
out_local = f"{WORK}/{OUT}"
summary = finetune(train_path, base, out_local, cfg, device="cuda")
summary["minutes"] = round((time.time() - t) / 60, 1)
summary["gpu"] = gpu
summary["data"] = DATA
with open(f"{out_local}/colony_finetune_summary.json", "w") as f:
    json.dump(summary, f, indent=2, default=str)

dest = f"{DRIVE}/{OUT}"
shutil.copytree(out_local, dest, dirs_exist_ok=True)
print(json.dumps({k: summary[k] for k in summary if k not in ("history",)}, indent=2, default=str)[:4000])
print("saved to", dest)
