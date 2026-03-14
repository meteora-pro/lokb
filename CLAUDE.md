# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Проект

lokb (Local Offline Knowledge Base) — персональная offline библиотека знаний на Rust. Объединяет публичные данные (Wikipedia, Wikidata, книги, статьи) и личные (чаты, заметки, фото, GPS) в единую поисковую систему с поддержкой FTS, semantic search и knowledge graph.

**Статус:** Early development. Минимальный CLI с файловым хранилищем и E2E Gherkin-тестами.

## Команды сборки и разработки

```bash
cargo build                    # сборка всего workspace
cargo build -p lokb-core       # сборка одного crate
cargo test                     # все тесты
cargo test -p lokb-core        # тесты одного crate
cargo test test_name           # один тест
cargo clippy --workspace       # линтер
cargo fmt --all                # форматирование
cargo run -p lokb-cli -- <cmd> # запуск CLI
```

## Архитектура

### Четырёхслойное хранение

```
RAW SOURCE → OPTIMIZED SOURCE → DERIVED → CACHE
```

- **RAW** — исходные файлы (ZIM, PDF, JSON dump). Удаляемые после обработки.
- **OPTIMIZED** — нормализованный текст в cluster-bundle zstd формате. Source of truth.
- **DERIVED** — индексы: chunks (LanceDB), FTS (Tantivy), embeddings (LanceDB), entities (LanceDB+SQLite), catalog (SQLite).
- **CACHE** — рендеренные документы, LRU eviction.

### Workspace crates (13 штук)

| Crate | Назначение |
|---|---|
| `lokb-core` | Типы, трейты, конфиг, budget manager |
| `lokb-storage` | Content Store + LanceDB + SQLite |
| `lokb-pipeline` | Pipeline framework (PipelineStep trait, executor) |
| `lokb-optimize` | RAW → OPTIMIZED оркестратор |
| `lokb-ingest` | OPTIMIZED → DERIVED оркестратор |
| `lokb-parsers` | Парсеры форматов (ZIM, PDF, EPUB, Telegram, RDF, Wikidata, ...) |
| `lokb-search` | Query engine, hybrid search, RRF, skills |
| `lokb-embed` | Embedding модели (ONNX/Candle) |
| `lokb-llm` | LLM бэкенды (Ollama, ONNX, Candle, OpenAI-compatible) |
| `lokb-graph` | Entity resolution, relations |
| `lokb-render` | Source Viewer (terminal ratatui + HTML) |
| `lokb-serve` | HTTP сервер (axum), будущий MCP |
| `lokb-cli` | CLI (clap) + TUI (ratatui) |

### Ключевые типы данных

- **Document** — иерархическая единица (Book→Chapter→Chunk, Conversation→Thread→Chunk)
- **Chunk** — единица поиска с optional embedding vector
- **Entity** — узел knowledge graph (Wikidata, NER)
- **Relation** — ребро графа
- **DataSource** — источник данных (Public или Personal с privacy levels)

### Pipeline

Composable цепочка PipelineStep: extractors → enrichers → transformers → writers. Конфигурируется через TOML. LLM-шаги опциональны с fallback = "skip".

## Ключевые зависимости

- **Storage:** lancedb, rusqlite (bundled), tantivy
- **RDF:** oxrdfio, oxttl, oxrdf (Oxigraph crates)
- **ML:** ort (ONNX Runtime, load-dynamic)
- **Compression:** zstd, blake3
- **Async:** tokio, axum
- **CLI:** clap (derive), ratatui
- **Data:** arrow-array, serde, uuid (v7), chrono

## Физическая структура данных

Всё хранится в `~/.local/share/lokb/` с подкаталогами: `raw/`, `source/`, `derived/`, `cache/`, `models/`.

## Принципы разработки

- **Offline-first** — всё работает без интернета
- **CLI-first** — composable unix tool, pipe-friendly (JSON/text output)
- **Embedding search опционален** — FTS и browse доступны сразу, vectors вычисляются в фоне
- **Privacy levels** (Public/Internal/Private/Secret) — личные данные не смешиваются с публичными при export
- Default embedding модель: `multilingual-e5-small` (384 dims, 120MB)
- **Тесты на английском** — BDD feature files и test code на английском языке
- **Conventional Commits** — `feat/`, `fix/`, `docs/`, `refactor/`, `test/`, `chore/`

## Claude Code Skills

| Skill | Команда | Описание |
|---|---|---|
| [solve-issue](.claude/skills/solve-issue/SKILL.md) | `/solve-issue <id>` | Полный цикл решения задачи: анализ → ветка → реализация → тесты → PR |
| [review-pr](.claude/skills/review-pr/SKILL.md) | `/review-pr <id>` | Code review PR: анализ diffs, pipeline check, inline комментарии |
| [fix-review-comments](.claude/skills/fix-review-comments/SKILL.md) | `/fix-review-comments <id>` | Исправить замечания из review и ответить на каждый комментарий |

Skills используют DevBoy MCP server (`mcp__dev-boy_lokb__`) для работы с GitHub API.

## E2E тесты

```bash
cargo test --test e2e -p lokb-cli   # запуск Gherkin-сценариев (cucumber-rs)
```

Feature файлы: `tests/features/*.feature`
Fixtures: `tests/fixtures/` (Wikipedia markdown, Telegram JSON)
Step definitions: `crates/lokb-cli/tests/e2e.rs`
