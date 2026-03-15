# ADR-008: FM-index for exact substring search

**Status:** Proposed
**Date:** 2026-03-15

## Context

lokb currently supports two search backends in DERIVED layer:
- **FTS (Tantivy BM25)** — token-based keyword search, fast ranking
- **Vectors (embeddings)** — semantic similarity via cosine distance

Neither supports **exact substring search** — finding arbitrary byte patterns across the entire corpus. This is needed for:
- Exact phrase search including punctuation (`"E=mc²"`, `"CRISPR-Cas9"`)
- Coordinate/number search (`"48.856°N 2.352°E"`)
- Code/formula patterns (`"O(n log n)"`)
- Cross-word patterns that tokenizers break (`"anti-disestablishmentarianism"`)

FM-index (Ferragina & Manzini, 2000) provides O(m) substring search on compressed text using Burrows-Wheeler Transform + backward search. It stores the entire corpus in ~25-50% of original size while supporting direct pattern matching without decompression.

## Decision

Add FM-index as an **optional EnrichmentStep** in the DERIVED layer. It is NOT a replacement for FTS or vectors — it's a third search modality.

### Architecture fit

```
DERIVED:
  ├── Chunks (LanceDB)         — always built
  ├── FTS Index (Tantivy)      — always built (cheap, minutes)
  ├── Embedding Vectors        — optional (expensive, hours)
  └── FM-index                 — optional (expensive, hours, large RAM)
```

FM-index follows the same patterns as other DERIVED indexes:
- Built by `EnrichmentStep` trait (ADR-005)
- Controlled by `BudgetManager` (ADR-003)
- Can be degraded or not built if budget insufficient

### Implementation plan

#### Block-based construction (manageable RAM)

Full Wikipedia (22GB text) cannot fit in RAM as a single suffix array. Solution: block-based FM-index.

```
Wikipedia 22GB text
  → split into 2GB blocks (11 blocks)
  → per block: SA-IS algorithm → BWT → FM-index (~24GB peak RAM)
  → serialize each block (~500MB)
  → total: ~5-6GB FM-index for all of English Wikipedia
  → peak RAM: ~24GB (single block at a time)
```

#### Rust crates

| Crate | Purpose | Status |
|---|---|---|
| `fm-index` | FM-index with FMIndexMultiPieces for multi-text search | Active, MIT/Apache-2.0 |
| `sux-rs` | Succinct data structures (Elias-Fano, rank/select) by S. Vigna | Authoritative |
| `vers_vecs` | Wavelet matrix, succinct bit vectors (used by fm-index) | Active |
| `sucds` | Pure Rust succinct primitives, no unsafe | Active |

`fm-index` with `FMIndexMultiPieces` is recommended — designed for searching across multiple text pieces (articles), supports count/locate queries.

#### Storage budget

| Corpus | Raw text | FM-index size | Build time | Build RAM |
|---|---|---|---|---|
| Simple English Wikipedia | ~400MB | ~100-200MB | ~10 min | ~4GB |
| English Wikipedia | ~22GB | ~5-10GB | ~hours | ~24GB/block |
| All sources combined | varies | ~50% of text | proportional | block-based |

#### Pre-built index distribution

For large corpora (Wikipedia EN), users download pre-built FM-index instead of building locally:

```bash
lokb source add wikipedia-en --raw ~/wiki.zim --format zim
# FTS built locally (30 min)
# FM-index downloaded: lokb index download wikipedia-en fm-index
```

### EnrichmentStep implementation

```rust
struct FmIndexBuilder {
    block_size: usize,  // 2GB default
    sampling_factor: u32, // SA sampling for faster locate (32-128)
}

impl EnrichmentStep for FmIndexBuilder {
    fn name(&self) -> &str { "fm_index" }
    fn enrichment_kind(&self) -> EnrichmentKind {
        EnrichmentKind::Custom("fm_index".into())
    }
    fn estimate(&self, chunk_count: u64) -> EnrichmentEstimate {
        EnrichmentEstimate {
            storage_overhead: chunk_count * 200,  // ~50% of text size
            compute_time: Duration::from_secs(chunk_count / 100), // rough
            needs_gpu: false,
            can_degrade: true,  // skip if budget insufficient
        }
    }
    fn supports_incremental(&self) -> bool { false } // full rebuild needed
}
```

### Search integration

Three-way hybrid search with RRF fusion:

```
query → FTS (BM25 ranked results)
      → FM-index (exact substring matches + positions)
      → Vectors (semantic similarity)
      → RRF fusion: score(doc) = Σ 1/(60 + rank_i)
      → final ranked results
```

New CLI option:
```bash
lokb search "E=mc²" --mode deep        # uses all three backends
lokb substring "CRISPR-Cas9"            # FM-index only, exact match
```

### Performance characteristics

| Operation | FM-index | FTS (Tantivy) | Vectors |
|---|---|---|---|
| Exact substring | **O(m)** — optimal | Tokenizer may break pattern | N/A |
| Word search | O(m) | **O(1) with posting list** — optimal | O(n) brute force |
| Semantic search | N/A | N/A | **Cosine similarity** — optimal |
| Article extraction | 3ms (slow, LF-mapping) | N/A | N/A |
| Index size (Wikipedia) | 5-10 GB | 2-5 GB | 20-35 GB |
| Build time | Hours | 30 min | 20+ hours CPU |
| Build RAM | 24 GB/block | 2 GB | 2 GB |

## Consequences

**Pros:**
- Unique capability: exact substring search not possible with FTS or vectors
- Compact: ~50% of text size (smaller than embedding vectors)
- O(m) search time regardless of corpus size
- Fits cleanly into existing EnrichmentStep architecture
- Optional: BudgetManager decides whether to build

**Cons:**
- High build cost: hours of CPU, 24GB RAM per 2GB block
- Not incremental: full rebuild needed when text changes
- Complex implementation: BWT, wavelet matrix, rank/select
- Article extraction from FM-index is 300x slower than zstd (use OPTIMIZED layer instead)
- Pre-built index distribution adds infrastructure

**Trade-off:** FM-index is most valuable for research/power users who need exact pattern matching. For typical Wikipedia search, FTS + vectors is sufficient.

## Alternatives considered

1. **Tantivy regex search:** Supports regex but over tokenized text, not raw bytes. Doesn't handle cross-token patterns.
2. **grep over OPTIMIZED:** Simple but O(n) — too slow for 22GB corpus. FM-index is O(m).
3. **Suffix array without compression:** 176GB for Wikipedia — not practical for offline tool.
4. **n-gram index:** Can approximate substring search but huge storage overhead and no exact match guarantee.
