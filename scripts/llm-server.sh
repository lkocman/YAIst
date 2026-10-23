#!/bin/bash
# Runs llama.cpp's server in a rootless podman container, so it works the same
# on every openSUSE (Leap, Tumbleweed, Slowroll, immutable variants) whether or
# not llama.cpp is packaged there. The model is downloaded on first start.
#
#   YAIST_MODEL      Hugging Face repo[:quant]  (default: Qwen3-4B-Instruct-2507 Q4_K_M)
#   YAIST_LLM_PORT   port on 127.0.0.1          (default: 8080)
#   YAIST_LLM_IMAGE  container image            (default: llama.cpp Vulkan build)
set -euo pipefail

MODEL="${YAIST_MODEL:-unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M}"
PORT="${YAIST_LLM_PORT:-8080}"
IMAGE="${YAIST_LLM_IMAGE:-ghcr.io/ggml-org/llama.cpp:server-vulkan}"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/yaist/models"

mkdir -p "$CACHE"

gpu=()
if [ -e /dev/dri ]; then
    gpu=(--device /dev/dri)
fi

exec podman run --rm --name yaist-llm \
    "${gpu[@]}" \
    --cap-drop all --security-opt no-new-privileges \
    -e LLAMA_CACHE=/models \
    -v "$CACHE":/models:Z \
    -p "127.0.0.1:$PORT:8080" \
    "$IMAGE" \
    -hf "$MODEL" \
    --host 0.0.0.0 --port 8080 \
    --jinja \
    -ngl 99 \
    -c 16384 \
    --cache-reuse 256
