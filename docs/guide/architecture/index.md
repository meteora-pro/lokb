# Architecture

## Четырёхслойное хранение

```
RAW SOURCE          → оригинал как скачан (PDF, ZIM, JSON dump)
  ↓ Composable Pipeline
OPTIMIZED SOURCE    → нормализованный текст, "хранилище идей" (zstd bundles)
  ↓ Ingestion
DERIVED             → индексы для поиска (chunks, vectors, FTS, graph)
  ↓ On-demand
CACHE               → рендеренные документы, распакованные блоки
```

| Слой | Что хранит | Budget | Пересоздание |
|---|---|---|---|
| **RAW** | Исходные файлы (ZIM, PDF, JSON) | 0-200 GB | — |
| **OPTIMIZED** | Нормализованный текст (zstd bundles) | 20-50 GB | Из RAW |
| **DERIVED** | Chunks, FTS, Vectors, Entities | 30-80 GB | Минуты — часы |
| **CACHE** | Рендеренные документы, query cache | 10-20 GB | Миллисекунды |

## Workspace crates

```
lokb/
├── lokb-core        # Типы, трейты, конфиг, budget manager
├── lokb-storage     # Content Store + LanceDB + SQLite
├── lokb-pipeline    # Pipeline framework (PipelineStep trait)
├── lokb-optimize    # RAW → OPTIMIZED оркестратор
├── lokb-ingest      # OPTIMIZED → DERIVED оркестратор
├── lokb-parsers     # ZIM, PDF, EPUB, Telegram, RDF, Wikidata...
├── lokb-search      # Query engine, hybrid search, RRF
├── lokb-embed       # Embedding модели (ONNX/Candle)
├── lokb-llm         # LLM бэкенды (Ollama, ONNX, OpenAI)
├── lokb-graph       # Entity resolution, relations
├── lokb-render      # Source Viewer (terminal + HTML)
├── lokb-serve       # HTTP сервер (axum), MCP
└── lokb-cli         # CLI (clap) + TUI (ratatui)
```

## Физическая структура

Все данные хранятся в `~/.local/share/lokb/`:

```
~/.local/share/lokb/
├── config.toml          # конфигурация
├── raw/                 # RAW SOURCE (удаляемые)
├── source/              # OPTIMIZED SOURCE (source of truth)
│   └── {name}/bundles/  # cluster-bundle zstd
├── derived/             # DERIVED
│   ├── lance/           # LanceDB (chunks, entities)
│   ├── fts/             # Tantivy
│   └── catalog.sqlite
├── cache/               # CACHE (LRU eviction)
└── models/              # Embedding модели
```

## Граф стоимости пересоздания

```
Дёшево (мс)        Средне (мин)        Дорого (часы)
──────────          ──────────          ──────────
Render HTML         Parse + Chunk       Embeddings
Decompress bundle   FTS index           NER extraction
Query cache         Entity import       LLM enrichment

← CACHE →           ← DERIVED(cheap) →  ← DERIVED(expensive) →
```
