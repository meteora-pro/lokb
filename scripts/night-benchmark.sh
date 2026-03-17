#!/bin/bash
# Night benchmark: download large Russian-language ZIM datasets,
# ingest into lokb, build all indices, run search benchmarks,
# and collect comprehensive metrics.
#
# Usage:
#   ./scripts/night-benchmark.sh [--skip-download] [--data-dir /path]
#
# Datasets (~12 GB download):
#   1. Russian Wikipedia (nopic)    — ~6 GB, ~2M articles
#   2. Russian Wikisource (nopic)   — ~4.3 GB, Russian literature
#   3. Russian Wiktionary (nopic)   — ~1.6 GB, dictionary
#
# Expected runtime: 4-8 hours depending on hardware.
# Disk usage: ~50-80 GB total (downloads + indices).

set -uo pipefail
# Don't exit on error - continue with other sources if one fails

# ── Configuration ─────────────────────────────────────────────────────
SKIP_DOWNLOAD=false
BASE_DIR="/tmp/lokb-night-bench"
LOKB="./target/release/lokb"
REPORT=""
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
REPORT_FILE="benchmark_report_${TIMESTAMP}.md"

for arg in "$@"; do
    case $arg in
        --skip-download) SKIP_DOWNLOAD=true ;;
        --data-dir=*) BASE_DIR="${arg#*=}" ;;
        --data-dir) shift; BASE_DIR="$1" ;;
    esac
done

DOWNLOAD_DIR="$BASE_DIR/downloads"
DATA_DIR="$BASE_DIR/data"
METRICS_DIR="$BASE_DIR/metrics"

mkdir -p "$DOWNLOAD_DIR" "$METRICS_DIR"

export LOKB_DATA_DIR="$DATA_DIR"
export LOKB_ZIM_BLOCK="${LOKB_ZIM_BLOCK:-20000}" # Block size for memory-bounded processing

# ── ZIM datasets ──────────────────────────────────────────────────────
declare -A DATASETS
DATASETS[wikipedia_ru]="https://download.kiwix.org/zim/wikipedia/wikipedia_ru_all_nopic_2026-01.zim"
DATASETS[wikisource_ru]="https://download.kiwix.org/zim/wikisource/wikisource_ru_all_nopic_2026-01.zim"
DATASETS[wiktionary_ru]="https://download.kiwix.org/zim/wiktionary/wiktionary_ru_all_nopic_2025-12.zim"

# ── Helper functions ──────────────────────────────────────────────────
log() {
    local msg="[$(date '+%H:%M:%S')] $1"
    echo "$msg"
    echo "$msg" >> "$METRICS_DIR/run.log"
}

report() {
    REPORT+="$1"$'\n'
}

save_report() {
    echo "$REPORT" > "$METRICS_DIR/$REPORT_FILE"
    # Also copy to project root for easy access
    cp "$METRICS_DIR/$REPORT_FILE" "./$REPORT_FILE"
    log "Report saved to ./$REPORT_FILE"
}

measure() {
    local label="$1"
    shift
    local start_time=$(date +%s)
    local start_rss=$(ps -o rss= -p $$ 2>/dev/null || echo 0)

    "$@" 2>&1 | tee -a "$METRICS_DIR/run.log"
    local exit_code=${PIPESTATUS[0]}

    local end_time=$(date +%s)
    local elapsed=$((end_time - start_time))
    local minutes=$((elapsed / 60))
    local seconds=$((elapsed % 60))

    log "  ⏱ $label: ${minutes}m ${seconds}s (exit: $exit_code)"
    echo "$label,$elapsed,$exit_code" >> "$METRICS_DIR/timings.csv"

    return $exit_code
}

du_mb() {
    du -sm "$1" 2>/dev/null | cut -f1
}

# ── Phase 0: Build ───────────────────────────────────────────────────
log "=== Phase 0: Build release binary ==="
if [ ! -f "$LOKB" ]; then
    measure "cargo build --release" cargo build --release -p lokb-cli
fi
log "Binary: $LOKB ($(du -sh "$LOKB" | cut -f1))"

report "# lokb Night Benchmark Report"
report ""
report "**Date:** $(date '+%Y-%m-%d %H:%M:%S')"
report "**Machine:** $(uname -m) / $(sysctl -n hw.ncpu 2>/dev/null || nproc) cores / $(sysctl -n hw.memsize 2>/dev/null | awk '{printf "%.0f GB", $1/1073741824}' || free -h 2>/dev/null | awk '/Mem:/{print $2}')"
report "**OS:** $(uname -s) $(uname -r)"
report "**Rust:** $(rustc --version)"
report ""

# ── Phase 1: Download ────────────────────────────────────────────────
log "=== Phase 1: Download ZIM datasets ==="
report "## Phase 1: Downloads"
report ""
report "| Dataset | Size | Time |"
report "|---------|------|------|"

for name in wiktionary_ru wikisource_ru wikipedia_ru; do
    url="${DATASETS[$name]}"
    filename=$(basename "$url")
    filepath="$DOWNLOAD_DIR/$filename"

    if [ -f "$filepath" ] && [ "$SKIP_DOWNLOAD" = true ]; then
        log "  Skip (exists): $filename"
        size=$(du -sh "$filepath" | cut -f1)
        report "| $name | $size | skipped |"
        continue
    fi

    if [ -f "$filepath" ]; then
        # Check if file is complete by comparing with remote
        local_size=$(stat -f%z "$filepath" 2>/dev/null || stat -c%s "$filepath" 2>/dev/null || echo 0)
        remote_size=$(curl -sI "$url" 2>/dev/null | grep -i content-length | awk '{print $2}' | tr -d '\r' || echo 0)
        if [ "$local_size" = "$remote_size" ] && [ "$local_size" -gt 0 ]; then
            log "  Already downloaded: $filename ($local_size bytes)"
            size=$(du -sh "$filepath" | cut -f1)
            report "| $name | $size | cached |"
            continue
        fi
        log "  Resuming download: $filename"
    fi

    log "  Downloading: $filename"
    dl_start=$(date +%s)
    curl -L -C - -o "$filepath" --progress-bar "$url"
    dl_end=$(date +%s)
    dl_time=$((dl_end - dl_start))

    size=$(du -sh "$filepath" | cut -f1)
    log "  Downloaded: $filename ($size in ${dl_time}s)"
    report "| $name | $size | ${dl_time}s |"
done

report ""
total_download=$(du -sh "$DOWNLOAD_DIR" | cut -f1)
log "Total downloads: $total_download"
report "**Total download size:** $total_download"
report ""

# ── Phase 2: Ingest ──────────────────────────────────────────────────
log "=== Phase 2: Ingest datasets ==="
rm -rf "$DATA_DIR"

report "## Phase 2: Ingestion"
report ""
report "| Source | Articles | Time | Content Size | FTS Size | Compression |"
report "|--------|----------|------|--------------|----------|-------------|"

for name in wiktionary_ru wikisource_ru wikipedia_ru; do
    url="${DATASETS[$name]}"
    filename=$(basename "$url")
    filepath="$DOWNLOAD_DIR/$filename"

    if [ ! -f "$filepath" ]; then
        log "  SKIP $name: file not found"
        continue
    fi

    log "  Ingesting: $name from $filename"
    ingest_start=$(date +%s)

    $LOKB source add "$name" \
        --raw "$filepath" \
        --format zim \
        --class public \
        --output json 2>&1 | tee "$METRICS_DIR/ingest_${name}.json" | tail -5

    ingest_end=$(date +%s)
    ingest_time=$((ingest_end - ingest_start))
    ingest_min=$((ingest_time / 60))
    ingest_sec=$((ingest_time % 60))

    # Collect metrics
    content_size=$(du_mb "$DATA_DIR/source/$name" 2>/dev/null || echo "?")
    fts_size=$(du_mb "$DATA_DIR/derived/fts" 2>/dev/null || echo "?")

    # Extract article count from JSON output
    articles=$(cat "$METRICS_DIR/ingest_${name}.json" 2>/dev/null | python3 -c "
import sys, json
for line in sys.stdin:
    line = line.strip()
    if line.startswith('{'):
        try:
            d = json.loads(line)
            print(d.get('documents', d.get('articles', '?')))
            break
        except: pass
" 2>/dev/null || echo "?")

    # Get ZIM file size for compression ratio
    zim_mb=$(du_mb "$filepath")
    if [ "$content_size" != "?" ] && [ "$zim_mb" != "0" ]; then
        ratio=$(echo "scale=2; $content_size / $zim_mb" | bc 2>/dev/null || echo "?")
    else
        ratio="?"
    fi

    log "  $name: ${articles} articles, ${ingest_min}m${ingest_sec}s, content=${content_size}MB, FTS=${fts_size}MB"
    report "| $name | $articles | ${ingest_min}m${ingest_sec}s | ${content_size}MB | ${fts_size}MB | ${ratio}x |"

    echo "${name},${articles},${ingest_time},${content_size},${fts_size}" >> "$METRICS_DIR/ingest_timings.csv"
done

report ""

# Storage overview after all ingestions
log "=== Storage status ==="
$LOKB storage status --format json 2>&1 | tee "$METRICS_DIR/storage_status.json"
$LOKB source list --format json 2>&1 | tee "$METRICS_DIR/source_list.json"

data_total=$(du_mb "$DATA_DIR")
report "**Total data directory:** ${data_total}MB"
report ""

# ── Phase 3: Build FM-index ──────────────────────────────────────────
log "=== Phase 3: Build FM-index (substring search) ==="
report "## Phase 3: FM-index Build"
report ""

fm_start=$(date +%s)
$LOKB build-index 2>&1 | tee "$METRICS_DIR/fm_build.log"
fm_end=$(date +%s)
fm_time=$((fm_end - fm_start))
fm_min=$((fm_time / 60))
fm_sec=$((fm_time % 60))

fm_size=$(du_mb "$DATA_DIR/derived/" 2>/dev/null || echo "?")
report "| Metric | Value |"
report "|--------|-------|"
report "| Build time | ${fm_min}m${fm_sec}s |"
report "| Derived dir size | ${fm_size}MB |"
report ""

# ── Phase 4: Search benchmarks ───────────────────────────────────────
log "=== Phase 4: Search benchmarks ==="
report "## Phase 4: Search Benchmarks"
report ""

# Russian search queries across different domains
declare -a QUERIES=(
    "квантовая физика"
    "Пушкин Евгений Онегин"
    "Москва столица России"
    "теория относительности Эйнштейн"
    "машинное обучение нейронные сети"
    "Великая Отечественная война 1941"
    "русский язык грамматика"
    "солнечная система планеты"
    "Достоевский Преступление и наказание"
    "химические элементы таблица Менделеева"
    "программирование алгоритмы"
    "Байкал глубочайшее озеро"
    "космонавтика Гагарин полёт"
    "математика теория чисел"
    "биология клетка ДНК"
)

# FTS search benchmark
report "### Full-Text Search (Tantivy BM25)"
report ""
report "| Query | Results | Time (ms) | Top Result |"
report "|-------|---------|-----------|------------|"

for query in "${QUERIES[@]}"; do
    search_start=$(python3 -c "import time; print(int(time.time()*1000))")

    result=$($LOKB search "$query" --format json --limit 5 2>/dev/null || echo '{"results":[]}')

    search_end=$(python3 -c "import time; print(int(time.time()*1000))")
    search_ms=$((search_end - search_start))

    count=$(echo "$result" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(len(d.get('results', [])))
except:
    print(0)
" 2>/dev/null || echo 0)

    top=$(echo "$result" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    r = d.get('results', [])
    if r:
        t = r[0].get('title', '?')[:40]
        print(t)
    else:
        print('-')
except:
    print('error')
" 2>/dev/null || echo "?")

    log "  FTS '$query': $count results in ${search_ms}ms"
    report "| $query | $count | $search_ms | $top |"

    echo "fts,$query,$count,$search_ms" >> "$METRICS_DIR/search_timings.csv"
done
report ""

# Substring search benchmark (FM-index)
report "### Substring Search (FM-index)"
report ""
report "| Pattern | Hits | Time (ms) |"
report "|---------|------|-----------|"

declare -a PATTERNS=(
    "Москва"
    "Пушкин"
    "революция"
    "квантовый"
    "нейронная сеть"
    "Достоевский"
    "кислород"
    "фотосинтез"
    "электричество"
    "теорема"
)

for pattern in "${PATTERNS[@]}"; do
    ss_start=$(python3 -c "import time; print(int(time.time()*1000))")

    result=$($LOKB substring "$pattern" --format json --limit 10 2>/dev/null || echo '{"hits":[],"count":0}')

    ss_end=$(python3 -c "import time; print(int(time.time()*1000))")
    ss_ms=$((ss_end - ss_start))

    count=$(echo "$result" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get('count', len(d.get('hits', []))))
except:
    print(0)
" 2>/dev/null || echo 0)

    log "  Substring '$pattern': $count hits in ${ss_ms}ms"
    report "| $pattern | $count | $ss_ms |"

    echo "substring,$pattern,$count,$ss_ms" >> "$METRICS_DIR/search_timings.csv"
done
report ""

# ── Phase 5: Read benchmarks ─────────────────────────────────────────
log "=== Phase 5: Read / entity benchmarks ==="
report "## Phase 5: Read & Entity Benchmarks"
report ""

# Entity search
report "### Entity Lookup"
report ""
report "| Entity | Found | Relations | Documents | Time (ms) |"
report "|--------|-------|-----------|-----------|-----------|"

declare -a ENTITIES=(
    "Россия"
    "Москва"
    "Пушкин"
    "Байкал"
    "Толстой"
)

for entity in "${ENTITIES[@]}"; do
    e_start=$(python3 -c "import time; print(int(time.time()*1000))")

    result=$($LOKB entity "$entity" --relations --documents --format json 2>/dev/null || echo '{}')

    e_end=$(python3 -c "import time; print(int(time.time()*1000))")
    e_ms=$((e_end - e_start))

    found=$(echo "$result" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    if d.get('name') or d.get('canonical_name'):
        print('yes')
    else:
        print('no')
except:
    print('no')
" 2>/dev/null || echo "no")

    relations=$(echo "$result" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(len(d.get('relations', [])))
except:
    print(0)
" 2>/dev/null || echo 0)

    docs=$(echo "$result" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(len(d.get('documents', [])))
except:
    print(0)
" 2>/dev/null || echo 0)

    log "  Entity '$entity': found=$found, relations=$relations, docs=$docs in ${e_ms}ms"
    report "| $entity | $found | $relations | $docs | $e_ms |"
done
report ""

# ── Phase 6: Incremental update (no changes) ─────────────────────────
log "=== Phase 6: Incremental update (no changes) ==="
report "## Phase 6: Incremental Update (no changes)"
report ""
report "| Source | Time | Result |"
report "|--------|------|--------|"

for name in wiktionary_ru wikisource_ru wikipedia_ru; do
    url="${DATASETS[$name]}"
    filename=$(basename "$url")
    filepath="$DOWNLOAD_DIR/$filename"

    [ ! -f "$filepath" ] && continue

    upd_start=$(date +%s)
    result=$($LOKB source update "$name" --raw "$filepath" 2>&1 || true)
    upd_end=$(date +%s)
    upd_time=$((upd_end - upd_start))
    upd_min=$((upd_time / 60))
    upd_sec=$((upd_time % 60))

    unchanged=$(echo "$result" | grep -o "Unchanged: [0-9]*" || echo "?")
    log "  Update $name: ${upd_min}m${upd_sec}s ($unchanged)"
    report "| $name | ${upd_min}m${upd_sec}s | $unchanged |"
done
report ""

# ── Phase 7: Disk usage summary ──────────────────────────────────────
log "=== Phase 7: Disk usage summary ==="
report "## Phase 7: Disk Usage"
report ""
report "| Component | Size |"
report "|-----------|------|"

downloads_total=$(du -sh "$DOWNLOAD_DIR" | cut -f1)
source_total=$(du -sh "$DATA_DIR/source" 2>/dev/null | cut -f1 || echo "0")
derived_total=$(du -sh "$DATA_DIR/derived" 2>/dev/null | cut -f1 || echo "0")
data_total_h=$(du -sh "$DATA_DIR" 2>/dev/null | cut -f1 || echo "0")

report "| ZIM downloads | $downloads_total |"
report "| Optimized content (source/) | $source_total |"
report "| Derived indices (derived/) | $derived_total |"
report "| **Total data dir** | **$data_total_h** |"
report ""

# Detailed derived breakdown
report "### Derived indices breakdown"
report ""
if [ -d "$DATA_DIR/derived" ]; then
    for d in "$DATA_DIR/derived"/*/; do
        if [ -d "$d" ]; then
            dname=$(basename "$d")
            dsize=$(du -sh "$d" | cut -f1)
            report "- $dname: $dsize"
        fi
    done
    # Also check for files directly in derived/
    for f in "$DATA_DIR/derived"/*; do
        if [ -f "$f" ]; then
            fname=$(basename "$f")
            fsize=$(du -sh "$f" | cut -f1)
            report "- $fname: $fsize"
        fi
    done
fi
report ""

# ── Phase 8: Compression analysis (for paper) ────────────────────────
log "=== Phase 8: Compression analysis ==="
report "## Phase 8: Compression Analysis"
report ""
report "### ZIM → Optimized Source compression"
report ""
report "| Dataset | ZIM size (MB) | Content size (MB) | Ratio | Notes |"
report "|---------|---------------|-------------------|-------|-------|"

for name in wiktionary_ru wikisource_ru wikipedia_ru; do
    url="${DATASETS[$name]}"
    filename=$(basename "$url")
    filepath="$DOWNLOAD_DIR/$filename"

    [ ! -f "$filepath" ] && continue

    zim_mb=$(du_mb "$filepath")
    content_mb=$(du_mb "$DATA_DIR/source/$name" 2>/dev/null || echo 0)

    if [ "$content_mb" -gt 0 ] && [ "$zim_mb" -gt 0 ]; then
        ratio=$(echo "scale=2; $zim_mb / $content_mb" | bc 2>/dev/null || echo "?")
        report "| $name | $zim_mb | $content_mb | ${ratio}:1 | ZIM(lzma) → zstd bundles |"
    else
        report "| $name | $zim_mb | $content_mb | - | |"
    fi
done
report ""

# Bundle analysis
report "### Bundle statistics"
report ""
if [ -d "$DATA_DIR/source" ]; then
    for src_dir in "$DATA_DIR/source"/*/; do
        [ ! -d "$src_dir" ] && continue
        src_name=$(basename "$src_dir")
        bundle_count=$(find "$src_dir" -name "*.bundle" 2>/dev/null | wc -l | tr -d ' ')
        total_size=$(du_mb "$src_dir" 2>/dev/null || echo 0)
        if [ "$bundle_count" -gt 0 ]; then
            avg_size=$((total_size / bundle_count))
            report "- **$src_name**: $bundle_count bundles, ${total_size}MB total, ~${avg_size}MB/bundle"
        else
            file_count=$(find "$src_dir" -type f 2>/dev/null | wc -l | tr -d ' ')
            report "- **$src_name**: $file_count files, ${total_size}MB total"
        fi
    done
fi
report ""

# ── Phase 9: Memory profiling ────────────────────────────────────────
log "=== Phase 9: Peak memory during search ==="
report "## Phase 9: Memory Profile"
report ""

# Run a search and measure peak RSS (macOS specific)
if command -v /usr/bin/time &>/dev/null; then
    mem_output=$( { /usr/bin/time -l $LOKB search "квантовая физика" --format json --limit 10 2>/dev/null; } 2>&1 || true)
    peak_rss=$(echo "$mem_output" | grep "maximum resident" | awk '{print $1}' || echo "?")
    if [ "$peak_rss" != "?" ]; then
        peak_mb=$((peak_rss / 1048576))
        report "- **Peak RSS during search:** ${peak_mb}MB"
    fi

    mem_output=$( { /usr/bin/time -l $LOKB substring "Москва" --format json --limit 10 2>/dev/null; } 2>&1 || true)
    peak_rss=$(echo "$mem_output" | grep "maximum resident" | awk '{print $1}' || echo "?")
    if [ "$peak_rss" != "?" ]; then
        peak_mb=$((peak_rss / 1048576))
        report "- **Peak RSS during substring search:** ${peak_mb}MB"
    fi
fi
report ""

# ── Final summary ────────────────────────────────────────────────────
end_timestamp=$(date '+%Y-%m-%d %H:%M:%S')
log "=== Benchmark complete at $end_timestamp ==="

report "---"
report ""
report "**Completed:** $end_timestamp"
report ""
report "*Generated by lokb night-benchmark.sh*"

save_report

log "All done! Report: ./$REPORT_FILE"
log "Metrics dir: $METRICS_DIR"
