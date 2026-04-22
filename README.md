# lokb — Local Offline Knowledge Base

[English](README.md) | [Русский](README.ru.md)

[![CI](https://github.com/meteora-pro/lokb/actions/workflows/ci.yml/badge.svg)](https://github.com/meteora-pro/lokb/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

A personal **offline-first knowledge base** written in Rust. Combines public data (Wikipedia, Wikidata, books, articles) and personal data (chats, notes, photos, GPS) into a unified search system with full-text search, semantic search, and a knowledge graph.

## Features

- **15 data formats**: ZIM (Wikipedia), EPUB, PDF, Markdown, HTML, Telegram, MBOX, GPX, EXIF, CSV, Wikidata JSON, Chrome/Firefox history, and more
- **Full-text search** (Tantivy BM25) — instant keyword search across millions of documents
- **FM-index** — exact substring search with context extraction
- **Semantic search** — multilingual-e5-small embeddings (384 dims, ONNX)
- **Hybrid search** — RRF fusion of FTS + vector search
- **Knowledge graph** — entities, relations, wikilink extraction
- **MCP Server** — Model Context Protocol for Claude Desktop / LLM integration
- **HTTP API** — REST endpoints for search, read, entities
- **TUI viewer** — terminal document reader with scroll, section nav, search highlight
- **Parallel ZIM pipeline** — channel-based Reader → Workers(N) → Writer architecture
- **Block-based processing** — bounded memory, processes millions of articles
- **Privacy levels** — Public/Internal/Private/Secret, personal data never leaks on export
- **Offline-first** — everything works without internet
- **CLI-first** — composable unix tool, pipe-friendly JSON/text output

## Quick Start

```bash
# Build
cargo build --release

# Add Wikipedia (ZIM file)
lokb source add wikipedia-ru --raw ~/wikipedia_ru.zim --format zim --class public --threads 8

# Add local markdown notes
lokb source add notes --raw ~/notes/ --format markdown-dir --class personal

# Search
lokb search "quantum physics"
lokb search "quantum physics" --format json --mode deep

# Read a document
lokb read wikipedia-ru:Quantum_computing

# Entity lookup
lokb entity "Albert Einstein" --relations --documents

# Start HTTP server
lokb serve --port 7890

# Start MCP server (for Claude Desktop)
lokb serve --mcp
```

## Architecture

Four-layer storage following [ADR-001](docs/architecture/adr/001-four-layer-storage.md):

```
RAW SOURCE → OPTIMIZED SOURCE → DERIVED → CACHE
(ZIM, PDF)   (zstd bundles)     (FTS, vectors, graph)  (rendered docs)
```

Parallel ingestion pipeline ([#46](https://github.com/meteora-pro/lokb/issues/46)):

```
[ZIM Reader]  ──channel──>  [Worker Pool (N)]  ──channel──>  [Writer]
   1 thread                  N threads                        1 thread
   I/O: decompress           CPU: HTML→MD, chunk              I/O: FTS, SQLite
```

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `lokb-core` | Types, traits, config |
| `lokb-pipeline` | Pipeline framework (OptimizeStep, EnrichmentStep) |
| `lokb-storage` | SQLite catalog, file content store |
| `lokb-parsers` | Format parsers (ZIM, EPUB, PDF, Telegram, GPX, ...) |
| `lokb-search` | Tantivy FTS, FM-index, vector search |
| `lokb-ingest` | Semantic chunker |
| `lokb-embed` | fastembed ONNX embeddings |
| `lokb-llm` | LLM backends (Ollama) |
| `lokb-graph` | Entity resolution |
| `lokb-render` | TUI document viewer (ratatui) |
| `lokb-serve` | HTTP API (axum) + MCP server |
| `lokb-tools` | Shared tool functions for CLI/MCP/HTTP |
| `lokb-cli` | CLI (clap) + parallel ZIM pipeline |

## CLI Commands

```
lokb source add <name> --raw <path> --format <fmt> --class <cls> [--threads N]
lokb source update <name> --raw <path>
lokb source status <name> [--format json]
lokb source delete <name>
lokb source list [--format json]
lokb search <query> [--mode quick|normal|deep] [--limit N] [--source <name>]
lokb read <source>:<doc_id> [--section <name>] [--highlight <text>]
lokb entity <name> [--relations] [--documents] [--format json]
lokb lookup "query"
lokb substring "pattern" [--limit N]
lokb enrich <source> --step <step> [--llm ollama:phi3]
lokb serve [--port 7890] [--mcp]
lokb storage status [--format json]
lokb export <output.tar.zst> [--include-personal]
lokb import <archive.tar.zst>
lokb build-index
```

## Supported Formats

| Format | Flag | Description |
|--------|------|-------------|
| `zim` | `--format zim` | Wikipedia/Kiwix offline dumps |
| `markdown-dir` | `--format markdown-dir` | Directory of .md files |
| `plaintext-dir` | `--format plaintext-dir` | Directory of .txt files |
| `html-dir` | `--format html-dir` | Directory of .html files |
| `epub` | `--format epub` | EPUB e-books |
| `pdf-dir` | `--format pdf-dir` | Directory of PDF files |
| `telegram-export` | `--format telegram-export` | Telegram JSON export |
| `mbox` | `--format mbox` | Email MBOX archives |
| `wikidata-json` | `--format wikidata-json` | Wikidata JSON dump |
| `gpx` | `--format gpx` | GPS track files |
| `exif-dir` | `--format exif-dir` | Photo EXIF metadata |
| `csv` / `tsv` | `--format csv` | CSV/TSV data files |
| `chrome-history` | `--format chrome-history` | Chrome browser history |
| `firefox-history` | `--format firefox-history` | Firefox browser history |

## Benchmarks

Tested on Apple M3 Max (16 cores, 48 GB RAM):

| Dataset | Size | Articles | Speed | Time |
|---------|------|----------|-------|------|
| Wiktionary RU | 1.6 GB ZIM | 1,456,534 | 880/s | 27 min |
| Wikisource RU | 4.3 GB ZIM | 200,000+ | 600/s | 5 min |
| FTS search latency | — | — | — | 26-63 ms |

## Configuration

Data directory: `~/.local/share/lokb/` (override with `LOKB_DATA_DIR`)

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `LOKB_DATA_DIR` | `~/.local/share/lokb` | Data directory |
| `LOKB_THREADS` | `num_cpus/2` | Worker threads for parallel ingestion |
| `LOKB_ZIM_BLOCK` | `20000` | Articles per commit block |

## Documentation

- [Architecture Decision Records](docs/architecture/adr/) — 8 ADRs covering storage, pipelines, data model
- [Technical Specification](docs/SPEC.md) — full design document
- [Online Docs](https://meteora-pro.github.io/lokb/) — Rspress documentation site

## Development

```bash
cargo build                           # build workspace
cargo test                            # all tests
cargo test --test e2e -p lokb-cli     # E2E Gherkin tests
cargo clippy --workspace              # linter
cargo fmt --all                       # format
cargo run -p lokb-cli -- <cmd>        # run CLI
```

## License

Apache 2.0
