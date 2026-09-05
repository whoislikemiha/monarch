#!/usr/bin/env bash
# MON-91 spike — download bge-small-en-v1.5 ONNX + tokenizer into .assets/.
# Idempotent: skips files that already exist at the expected size.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
assets="$here/.assets"
mkdir -p "$assets"

# Prefer Xenova's ONNX export (reliably packaged for transformers.js / ort).
base="https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main"

fetch() {
    local url="$1"
    local out="$2"
    if [[ -s "$out" ]]; then
        echo "[skip] $out already present ($(stat -c%s "$out" 2>/dev/null || stat -f%z "$out") bytes)"
        return 0
    fi
    echo "[fetch] $url -> $out"
    curl -fL --retry 3 -o "$out.tmp" "$url"
    mv "$out.tmp" "$out"
}

fetch "$base/onnx/model.onnx"       "$assets/model.onnx"
fetch "$base/tokenizer.json"        "$assets/tokenizer.json"
fetch "$base/tokenizer_config.json" "$assets/tokenizer_config.json"
fetch "$base/config.json"           "$assets/config.json"

echo "done. .assets/ contents:"
ls -lh "$assets"
