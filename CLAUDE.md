# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Проект

lokb (Local Offline Knowledge Base) — персональная offline библиотека знаний на Rust. Объединяет публичные данные (Wikipedia, Wikidata, книги, статьи) и личные (чаты, заметки, фото, GPS) в единую поисковую систему с поддержкой FTS, semantic search и knowledge graph.

**Статус:** Early development. Минимальный CLI с файловым хранилищем и E2E Gherkin-тестами.

## Команды сборки и разработки

```bash
cargo build                    # сборка всего workspace
cargo test                     # все тесты
cargo test --test e2e -p lokb-cli  # E2E Gherkin-тесты
cargo test test_name           # один тест
cargo clippy --workspace       # линтер
cargo fmt --all                # форматирование
cargo run -p lokb-cli -- <cmd> # запуск CLI
```

## Архитектура

Подробная документация: `docs/architecture/adr/` (7 ADR).

### Четырёхслойное хранение (ADR-001)

```
RAW SOURCE → OPTIMIZED SOURCE → DERIVED → CACHE
```

- **RAW** — исходные файлы (ZIM, PDF, JSON dump). Удаляемые после обработки.
- **OPTIMIZED** — нормализованный текст в cluster-bundle zstd формате. Source of truth. Неприкосновенен.
- **DERIVED** — индексы: chunks (LanceDB), FTS (Tantivy), embeddings (LanceDB), entities (LanceDB+SQLite), catalog (SQLite). Поддерживает деградацию при нехватке бюджета (ADR-003).
- **CACHE** — рендеренные документы, LRU eviction.

### Три pipeline (ADR-002)

- **Optimize Pipeline** (RAW → OPTIMIZED): сжать, извлечь суть, унифицировать. Trait: `OptimizeStep`.
- **Enrichment Pipeline** (OPTIMIZED → DERIVED): расширить, построить индексы, связи. Trait: `EnrichmentStep`.
- **Cross-source Pipeline** (DERIVED × N → unified): связать данные между источниками. Trait: `CrossSourceStep`.

### Текущая структура (actual)

Сейчас весь код в одном crate `lokb-cli`. Целевая архитектура — 13 crates (см. README.md §13, Phase 0 roadmap [#6](https://github.com/meteora-pro/lokb/issues/6)).

| Crate (actual) | Назначение |
|---|---|
| `lokb-cli` | CLI (clap) + файловое хранилище + текстовый поиск |

### Ключевые типы данных (ADR-004)

- **DataSource** — источник данных (Public или Personal с privacy levels)
- **Document** — иерархическая единица (Book→Chapter→Chunk, Conversation→Thread→Chunk)
- **Chunk** — единица поиска с optional embedding vector
- **Entity** — узел knowledge graph (Wikidata, NER)
- **Relation** — ребро графа

### Физическая структура данных

Всё хранится в `~/.local/share/lokb/` (переопределяется через `LOKB_DATA_DIR`):

- `sources/` — конфиги источников (JSON)
- `source/` — контент (OPTIMIZED bundles)
- `derived/` — индексы
- `cache/` — рендеренные документы

### Реализованные CLI команды

```bash
lokb source add <name> --raw <path> --format <fmt> --class <cls> [--output json]
lokb source update <name> --raw <path>
lokb source status <name> [--format json]
lokb source delete <name>
lokb source list [--format json]
lokb search <query> [--format json] [--mode quick|normal|deep] [--limit N] [--source <name>] [--personal-only] [--public-only]
lokb read <source>:<doc_id> [--section <name>]
lokb entity <name> [--relations] [--documents] [--format json]
lokb lookup "query"
lokb enrich <source> --step summarize [--llm ollama:phi3]
lokb serve [--port 7890]
lokb storage status [--format json]
lokb export <output> [--include-personal]
```

Поддерживаемые форматы: `markdown-dir`, `plaintext-dir`, `html-dir`, `telegram-export`, `epub`, `pdf-dir`, `zim`, `wikidata-json`, `mbox`, `gpx`, `exif-dir`, `csv`, `tsv`

Поддерживаемые форматы: `markdown-dir`, `telegram-export`.

## Ключевые зависимости (planned)

- **Storage:** lancedb, rusqlite (bundled), tantivy
- **RDF:** oxrdfio, oxttl, oxrdf (Oxigraph crates)
- **ML:** ort (ONNX Runtime, load-dynamic)
- **Compression:** zstd, blake3
- **Async:** tokio, axum
- **CLI:** clap (derive), ratatui
- **Data:** arrow-array, serde, uuid (v7), chrono

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

## Документация

Документация на [Rspress](https://rspress.dev/) с i18n (English primary, Russian) и автодеплоем на GitHub Pages.

```bash
cd docs && pnpm install && pnpm dev   # локальная разработка
cd docs && pnpm build                  # сборка
```

Структура: `docs/guide/en/` — English (primary), `docs/guide/ru/` — Russian.
Деплой: `.github/workflows/deploy-docs.yml` при push в main (path: `docs/**`).
ADR: `docs/architecture/adr/` — Architecture Decision Records (7 штук).
