#!/usr/bin/env bash
# Start everything the colony's AI needs:
#   1. llama.cpp's server on 127.0.0.1:8766 with LFM3-1.7B (Q4_0 GGUF) for chatting with colonists.
#      (LM Studio's bundled engines don't support the lfm3 architecture yet; Homebrew llama.cpp does.)
#   2. The brain server on 127.0.0.1:8765 (Laya decisions on the CPU + chat relay).
# Override LFM3_GGUF / LLM_PORT / PORT if needed.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p ../logs

LFM3_GGUF="${LFM3_GGUF:-$HOME/.lmstudio/models/LiquidAI/LFM3-1.7B-GGUF/LFM3-1.7B-QAD-Q4_0.gguf}"
LLM_PORT="${LLM_PORT:-8766}"
# The chat model (4-bit LFM3-1.7B) runs on the GPU (~80 tokens/s) and Laya on the CPU: measured
# in a 50-person colony, that split keeps the game's frame rate best (Laya on the GPU, or both on
# the CPU, cost more). LLM_GPU_LAYERS=0 (+ LLM_THREADS) puts the chat model on the CPU.
LLM_GPU_LAYERS="${LLM_GPU_LAYERS:-99}"

if ! curl -sf "http://127.0.0.1:${LLM_PORT}/health" >/dev/null 2>&1; then
  if command -v llama-server >/dev/null && [ -f "$LFM3_GGUF" ]; then
    llama-server -m "$LFM3_GGUF" --host 127.0.0.1 --port "$LLM_PORT" -c 8192 --jinja -ngl "$LLM_GPU_LAYERS" -t "${LLM_THREADS:-2}" \
      > ../logs/llama_server.log 2>&1 &
    trap 'kill $! 2>/dev/null || true' EXIT
  else
    echo "warning: llama-server or $LFM3_GGUF missing; chat will be unavailable (brew install llama.cpp)"
  fi
fi

# Laya checkpoint: the newest fine-tuned colony model in ../models/ if there is one, else base Laya.
if [ -z "${LAYA_MODEL:-}" ]; then
  latest=$(ls -d ../models/laya_colony_* 2>/dev/null | sort -V | tail -1 || true)
  if [ -n "$latest" ] && [ -f "$latest/rl_agent_config.json" ]; then
    export LAYA_MODEL="$(cd "$latest" && pwd)"
  fi
fi
# Laya on the CPU by default (3 threads, LAYA_THREADS): on the GPU each decision competes with the
# game's rendering; on the CPU it takes ~0.4 s and the game keeps its frame rate. LAYA_DEVICE=mps
# for the GPU.
export LAYA_DEVICE="${LAYA_DEVICE:-cpu}"
echo "Laya model: ${LAYA_MODEL:-convaiinnovations/laya (base)} on ${LAYA_DEVICE}"

export LLM_URL="${LLM_URL:-http://127.0.0.1:${LLM_PORT}/v1/chat/completions}"
.venv/bin/uvicorn server:app --host 127.0.0.1 --port "${PORT:-8765}" --log-level warning
