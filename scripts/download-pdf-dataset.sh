#!/bin/bash
# Download open-access PDF papers from arXiv for benchmark testing.
#
# Usage:
#   ./scripts/download-pdf-dataset.sh [count]
#   Default: 100 papers (~200MB)
#
# Sources: arXiv CS.AI/CS.CL/CS.LG top papers

set -euo pipefail

COUNT="${1:-100}"
OUTPUT_DIR="/tmp/lokb-bench/pdf-dataset"
mkdir -p "$OUTPUT_DIR"

EXISTING=$(find "$OUTPUT_DIR" -name "*.pdf" 2>/dev/null | wc -l | tr -d ' ')
if [ "$EXISTING" -ge "$COUNT" ]; then
    echo "Dataset already has $EXISTING papers (requested $COUNT). Skipping download."
    du -sh "$OUTPUT_DIR"
    exit 0
fi

echo "Downloading $COUNT arXiv papers to $OUTPUT_DIR..."

# Well-known open-access papers (CS.AI, CS.CL, CS.LG)
PAPERS=(
    "1706.03762"  # Attention Is All You Need
    "1810.04805"  # BERT
    "2005.14165"  # GPT-3
    "1301.3781"   # Word2Vec
    "1409.0473"   # Neural Machine Translation (Bahdanau)
    "1412.6980"   # Adam Optimizer
    "1502.03167"  # Batch Normalization
    "1512.03385"  # ResNet
    "1406.2661"   # GANs
    "1710.10903"  # GAT
    "1607.06450"  # Layer Normalization
    "1609.08144"  # WaveNet
    "1803.02999"  # SQuAD 2.0
    "1910.10683"  # T5
    "2010.11929"  # ViT
    "2103.14030"  # Swin Transformer
    "2106.09685"  # LoRA
    "2203.02155"  # InstructGPT
    "2302.13971"  # LLaMA
    "2307.09288"  # LLaMA 2
    "1404.5997"   # Dropout as Bayesian
    "1505.04597"  # U-Net
    "1506.01497"  # Faster R-CNN
    "1506.02640"  # YOLO
    "1511.06434"  # DCGAN
    "1312.6114"   # VAE
    "1409.1556"   # VGGNet
    "1507.05717"  # Deformable Convolutions
    "1511.07122"  # Distillation
    "1605.07146"  # Data Augmentation
)

DOWNLOADED=0
for paper_id in "${PAPERS[@]}"; do
    if [ "$DOWNLOADED" -ge "$COUNT" ]; then
        break
    fi
    OUTPUT_FILE="$OUTPUT_DIR/${paper_id}.pdf"
    if [ -f "$OUTPUT_FILE" ]; then
        DOWNLOADED=$((DOWNLOADED + 1))
        continue
    fi
    echo "  Downloading $paper_id..."
    if curl -sL -o "$OUTPUT_FILE" "https://arxiv.org/pdf/${paper_id}" 2>/dev/null; then
        DOWNLOADED=$((DOWNLOADED + 1))
    else
        echo "  Warning: failed to download $paper_id"
        rm -f "$OUTPUT_FILE"
    fi
    sleep 1  # respect arXiv rate limits
done

echo ""
TOTAL=$(find "$OUTPUT_DIR" -name "*.pdf" 2>/dev/null | wc -l | tr -d ' ')
echo "Downloaded: $TOTAL papers"
du -sh "$OUTPUT_DIR"
