#!/usr/bin/env bash
# Start everything the colony's AI needs:
#   1. llama.cpp's server on 127.0.0.1:8766 with LFM3-1.7B (Q4_0 GGUF) for chatting with colonists.
#      (LM Studio's bundled engines don't support the lfm3 architecture yet; Homebrew llama.cpp does.)
#   2. The brain server on 127.0.0.1:8765 (Laya decisions + chat relay).
# Override LFM3_GGUF / LLM_PORT / PORT if needed.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p ../logs

LFM3_GGUF="${LFM3_GGUF:-$HOME/.lmstudio/models/LiquidAI/LFM3-1.7B-GGUF/LFM3-1.7B-QAD-Q4_0.gguf}"
LLM_PORT="${LLM_PORT:-8766}"

if ! curl -sf "http://127.0.0.1:${LLM_PORT}/health" >/dev/null 2>&1; then
  if command -v llama-server >/dev/null && [ -f "$LFM3_GGUF" ]; then
    llama-server -m "$LFM3_GGUF" --host 127.0.0.1 --port "$LLM_PORT" -c 8192 --jinja -ngl 99 \
      > ../logs/llama_server.log 2>&1 &
    trap 'kill $! 2>/dev/null || true' EXIT
  else
    echo "warning: llama-server or $LFM3_GGUF missing; chat will be unavailable (brew install llama.cpp)"
  fi
fi

export LLM_URL="${LLM_URL:-http://127.0.0.1:${LLM_PORT}/v1/chat/completions}"
.venv/bin/uvicorn server:app --host 127.0.0.1 --port "${PORT:-8765}" --log-level warning
