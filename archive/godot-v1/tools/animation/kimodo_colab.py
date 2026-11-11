"""Generate the colony's animations with NVIDIA Kimodo on a Colab GPU.

Runtime: T4 GPU **with High-RAM** (Runtime > Change runtime type). The 8B text encoder runs on the
CPU (~16 GB RAM); the motion model itself uses < 3 GB of GPU memory.
Needs: a Hugging Face token whose account has access to meta-llama/Meta-Llama-3-8B-Instruct,
saved in Colab Secrets as HF_TOKEN (notebook access on).

Notebook cells:
    from google.colab import drive; drive.mount('/content/drive')
    !pip -q install "kimodo @ git+https://github.com/nv-tlabs/kimodo.git"
    %run /content/drive/MyDrive/colony_anim/kimodo_colab.py

Reads  MyDrive/colony_anim/prompts.json
Writes MyDrive/colony_anim/<OUT>/<name>_s0.bvh, <name>_s1.bvh  (standard T-pose rest, 30 fps)
Then on the Mac:  Blender -b -P tools/blender/villager.py -- --out assets/models --bvh <that folder>
Env overrides: OUT (default bvh_v1), SAMPLES (default 2), STEPS (default 100), ONLY (comma list).
"""

import json
import os
import time

DRIVE = "/content/drive/MyDrive/colony_anim"
OUT = os.path.join(DRIVE, os.environ.get("OUT", "bvh_v1"))
SAMPLES = int(os.environ.get("SAMPLES", "2"))
STEPS = int(os.environ.get("STEPS", "100"))
ONLY = [s for s in os.environ.get("ONLY", "").split(",") if s]

# Token from Colab Secrets (never printed or written anywhere).
try:
    from google.colab import userdata
    token = userdata.get("HF_TOKEN")
except Exception as e:
    raise SystemExit(f"Add your Hugging Face token to Colab Secrets as HF_TOKEN first ({e})")
os.environ["HF_TOKEN"] = token
from huggingface_hub import login  # noqa: E402
login(token=token, add_to_git_credential=False)

os.environ.setdefault("TEXT_ENCODER_DEVICE", "cpu")   # 8B encoder on CPU: fits a T4 runtime with High-RAM
os.environ.setdefault("TEXT_ENCODER_MODE", "local")

import torch  # noqa: E402
from kimodo import load_model  # noqa: E402
from kimodo.exports.bvh import save_motion_bvh  # noqa: E402
from kimodo.skeleton import SOMASkeleton30, global_rots_to_local_rots  # noqa: E402
from kimodo.tools import seed_everything  # noqa: E402

assert torch.cuda.is_available(), "No GPU: Runtime > Change runtime type > T4 GPU"
device = "cuda:0"
print("GPU:", torch.cuda.get_device_name(0))

t = time.time()
model = load_model("kimodo-soma-rp", device=device)
print(f"model loaded in {time.time() - t:.0f}s, fps {model.fps}")
skeleton = model.skeleton
if isinstance(skeleton, SOMASkeleton30):
    skeleton = skeleton.somaskel77.to(device)

clips = json.load(open(os.path.join(DRIVE, "prompts.json")))["clips"]
os.makedirs(OUT, exist_ok=True)
for i, clip in enumerate(clips):
    if ONLY and clip["name"] not in ONLY:
        continue
    t = time.time()
    seed_everything(1000 + i)
    out = model([clip["prompt"]], [int(clip["seconds"] * model.fps)], constraint_lst=[],
                num_denoising_steps=STEPS, num_samples=SAMPLES, multi_prompt=True,
                post_processing=True, return_numpy=True)
    for s in range(int(out["posed_joints"].shape[0])):
        pos = torch.from_numpy(out["posed_joints"][s]).to(device)
        rots = torch.from_numpy(out["global_rot_mats"][s]).to(device)
        local = global_rots_to_local_rots(rots, skeleton)
        save_motion_bvh(os.path.join(OUT, f"{clip['name']}_s{s}.bvh"), local, pos[:, skeleton.root_idx, :],
                        skeleton=skeleton, fps=model.fps, standard_tpose=True)
    print(f"{clip['name']:12s} {SAMPLES} takes in {time.time() - t:.0f}s")
print("done ->", OUT)
