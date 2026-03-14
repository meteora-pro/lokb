# Architecture

> Detailed architecture is documented in [ADR-001 through ADR-007](https://github.com/meteora-pro/lokb/tree/main/docs/architecture/adr).

## Four-layer storage (ADR-001)

```
RAW SOURCE          → original files (PDF, ZIM, JSON dump)
  ↓ Optimize Pipeline (format-specific, compresses)
OPTIMIZED SOURCE    → normalized text, "idea storage" (zstd bundles)
  ↓ Enrichment Pipeline (format-agnostic, expands)
DERIVED             → search indexes (chunks, vectors, FTS, graph)
  ↓ On-demand
CACHE               → rendered documents, decompressed blocks
```

| Layer | What it stores | Budget | Rebuild cost |
|---|---|---|---|
| **RAW** | Original files (ZIM, PDF, JSON) | 0-200 GB | — |
| **OPTIMIZED** | Normalized text (zstd bundles). **Immutable source of truth.** | 20-50 GB | From RAW |
| **DERIVED** | Chunks, FTS, Vectors, Entities. Supports degradation. | 30-80 GB | Minutes to hours |
| **CACHE** | Rendered documents, query cache. LRU eviction. | 10-20 GB | Milliseconds |

## Dual pipeline (ADR-002)

Two pipelines with opposite goals:

| | Optimize Pipeline | Enrichment Pipeline |
|---|---|---|
| **Direction** | Data gets smaller | Data gets larger |
| **Goal** | Compress, extract essence | Expand, build indexes & links |
| **Trait** | `OptimizeStep` | `EnrichmentStep` |
| **Input** | RAW (format-specific) | OPTIMIZED (format-agnostic text) |
| **Output** | OPTIMIZED (text) | DERIVED (indexes, graph, vectors) |
| **Metrics** | compression_ratio, compute_time | storage_overhead, compute_time |

A third pipeline — **Cross-source** (`CrossSourceStep`) — links data between sources via entity resolution, spatio-temporal linking, and semantic graph building.

## Current vs planned crate structure

Currently all code lives in a single crate `lokb-cli`. The planned 13-crate workspace ([Phase 0, #6](https://github.com/meteora-pro/lokb/issues/6)):

```
lokb/
├── lokb-core        # Types, traits, config, budget manager
├── lokb-storage     # Content Store + LanceDB + SQLite
├── lokb-pipeline    # Pipeline framework (OptimizeStep, EnrichmentStep traits)
├── lokb-optimize    # RAW → OPTIMIZED orchestrator
├── lokb-ingest      # OPTIMIZED → DERIVED orchestrator
├── lokb-parsers     # ZIM, PDF, EPUB, Telegram, RDF, Wikidata...
├── lokb-search      # Query engine, hybrid search, RRF
├── lokb-embed       # Embedding models (ONNX/Candle)
├── lokb-llm         # LLM backends (Ollama, ONNX, OpenAI)
├── lokb-graph       # Entity resolution, relations
├── lokb-render      # Source Viewer (terminal + HTML)
├── lokb-serve       # HTTP server (axum), MCP
└── lokb-cli         # CLI (clap) + TUI (ratatui)
```

## Data directory

All data is stored in `~/.local/share/lokb/` (override via `LOKB_DATA_DIR`):

```
~/.local/share/lokb/
├── sources/             # Source configs (JSON)
├── source/              # OPTIMIZED content (bundles)
│   └── {name}/
├── derived/             # Indexes
│   ├── lance/           # LanceDB (chunks, entities)
│   ├── fts/             # Tantivy
│   └── catalog.sqlite
├── cache/               # LRU eviction
└── models/              # Embedding models
```

## Architecture Decision Records

| ADR | Topic |
|---|---|
| [001](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/001-four-layer-storage.md) | Four-layer storage |
| [002](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/002-dual-pipeline.md) | Dual pipeline: Optimize + Enrichment + Cross-source |
| [003](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/003-resource-management.md) | Resource management: Budget, Compute Queue, Lifecycle |
| [004](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/004-core-entities.md) | Core entities: DataSource, Document, Chunk, Entity, Relation |
| [005](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/005-extension-model.md) | Extension model: 7 trait-based plugin points |
| [006](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/006-incremental-loading-deduplication.md) | Incremental loading and deduplication |
| [007](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/007-datasource-pipelines-reference.md) | DataSource pipelines: 11 concrete sources |
