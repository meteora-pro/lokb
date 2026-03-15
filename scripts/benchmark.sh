#!/bin/bash
# End-to-end benchmark for lokb with Wikipedia dataset.
#
# Usage:
#   ./scripts/benchmark.sh [article_count]
#
# Default: all articles from Simple English Wikipedia dump.
# Requires: /tmp/lokb-bench/simplewiki.xml.bz2 (download first)

set -euo pipefail

DUMP="/tmp/lokb-bench/simplewiki.xml.bz2"
WIKI_DIR="/tmp/lokb-bench/wiki-md"
DATA_DIR="/tmp/lokb-bench/data"
LIMIT="${1:-}"
LOKB="./target/release/lokb"

if [ ! -f "$DUMP" ]; then
    echo "Downloading Simple English Wikipedia dump..."
    mkdir -p /tmp/lokb-bench
    curl -L -o "$DUMP" \
        "https://dumps.wikimedia.org/simplewiki/latest/simplewiki-latest-pages-articles-multistream.xml.bz2"
fi

echo "=== Step 1: Extract articles ==="
rm -rf "$WIKI_DIR"
mkdir -p "$WIKI_DIR"
EXTRACT_ARGS="$WIKI_DIR"
if [ -n "$LIMIT" ]; then
    EXTRACT_ARGS="$WIKI_DIR --limit $LIMIT"
fi
time bzcat "$DUMP" | python3 scripts/wiki-to-markdown.py $EXTRACT_ARGS

ARTICLE_COUNT=$(ls "$WIKI_DIR" | wc -l | tr -d ' ')
WIKI_SIZE=$(du -sh "$WIKI_DIR" | cut -f1)
echo "Extracted: $ARTICLE_COUNT articles, $WIKI_SIZE"

echo ""
echo "=== Step 2: source add ==="
rm -rf "$DATA_DIR"
export LOKB_DATA_DIR="$DATA_DIR"
time $LOKB source add simplewiki --raw "$WIKI_DIR" --format markdown-dir --class public

echo ""
echo "=== Step 3: storage status ==="
$LOKB storage status

echo ""
echo "=== Step 4: search benchmarks ==="
for query in "quantum computing" "Eiffel Tower history" "population growth" "machine learning" "solar system planets"; do
    echo -n "  search '$query': "
    time $LOKB search "$query" --format json 2>/dev/null | python3 -c "import sys,json; r=json.load(sys.stdin)['results']; print(f'{len(r)} results')" 2>/dev/null
done

echo ""
echo "=== Step 5: source update (no changes) ==="
time $LOKB source update simplewiki --raw "$WIKI_DIR"

echo ""
echo "=== Step 6: source status ==="
$LOKB source status simplewiki

echo ""
echo "=== Step 7: source delete ==="
time $LOKB source delete simplewiki

echo ""
echo "=== Cleanup ==="
rm -rf "$DATA_DIR"
echo "Done."
