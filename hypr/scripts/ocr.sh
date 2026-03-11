#!/bin/bash
set -euo pipefail
export OCR_ANNOTATE_MODE=overlay
export OCR_TRANSLATE_BACKEND=qwen3_genai
export OCR_USE_LAIS=1
export OCR_QWEN_NO_THINK=1
 export OCR_QWEN_NO_THINK_MODE=soft
BIN="$HOME/data/model/ocr_blast/target/release/ocr_blast"

if ! command -v grim >/dev/null; then
  echo "grim not found" >&2
  exit 1
fi
if ! command -v slurp >/dev/null; then
  echo "slurp not found" >&2
  exit 1
fi
if [ ! -x "$BIN" ]; then
  echo "binary not found: $BIN" >&2
  exit 1
fi

export OCR_ENGINE_PY="$HOME/data/model/.venv/bin/python"
export OCR_ENGINE_SCRIPT="$HOME/data/model/run_paddleocr_openvino_npu.py"
export OV_DEVICE=${OV_DEVICE:-NPU}
export OV_DEVICE_DET=${OV_DEVICE_DET:-$OV_DEVICE}
export OV_DEVICE_REC=${OV_DEVICE_REC:-$OV_DEVICE}
export OCR_DET_SCALE=${OCR_DET_SCALE:-1.0}
export OCR_REC_DYNAMIC=${OCR_REC_DYNAMIC:-1}
export OCR_REC_DYNAMIC_STRICT=${OCR_REC_DYNAMIC_STRICT:-1}
export OCR_REC_MAX_WIDTH=${OCR_REC_MAX_WIDTH:-4096}

if [ $# -ge 1 ] && [ -f "$1" ]; then
  exec "$BIN" --image "$1"
else
  exec "$BIN"
fi
