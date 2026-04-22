# lokb — Локальная Офлайн База Знаний

[English](README.md) | [Русский](README.ru.md)

[![CI](https://github.com/meteora-pro/lokb/actions/workflows/ci.yml/badge.svg)](https://github.com/meteora-pro/lokb/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

Персональная **офлайн-первая база знаний** на Rust. Объединяет публичные данные (Wikipedia, Wikidata, книги, статьи) и личные данные (чаты, заметки, фото, GPS) в единую поисковую систему с полнотекстовым поиском, семантическим поиском и графом знаний.

## Возможности

- **15 форматов данных**: ZIM (Wikipedia), EPUB, PDF, Markdown, HTML, Telegram, MBOX, GPX, EXIF, CSV, Wikidata JSON, история Chrome/Firefox и другие
- **Полнотекстовый поиск** (Tantivy BM25) — мгновенный поиск по ключевым словам среди миллионов документов
- **FM-индекс** — точный подстроковый поиск с извлечением контекста
- **Семантический поиск** — multilingual-e5-small embeddings (384 dims, ONNX)
- **Гибридный поиск** — RRF-слияние FTS + vector search
- **Граф знаний** — сущности, связи, извлечение wikilinks
- **MCP Server** — Model Context Protocol для интеграции с Claude Desktop / LLM
- **HTTP API** — REST-эндпоинты для поиска, чтения, сущностей
- **TUI просмотр** — терминальный читатель документов со скроллом, навигацией по секциям, подсветкой поиска
- **Параллельный ZIM pipeline** — архитектура Reader → Workers(N) → Writer на каналах
- **Блочная обработка** — ограниченная память, обработка миллионов статей
- **Уровни приватности** — Public/Internal/Private/Secret, личные данные не утекают при экспорте
- **Офлайн-первый** — всё работает без интернета
- **CLI-первый** — композируемый unix-инструмент, pipe-friendly JSON/text вывод

## Быстрый старт

```bash
# Сборка
cargo build --release

# Добавить Wikipedia (ZIM файл)
lokb source add wikipedia-ru --raw ~/wikipedia_ru.zim --format zim --class public --threads 8

# Добавить локальные markdown заметки
lokb source add notes --raw ~/notes/ --format markdown-dir --class personal

# Поиск
lokb search "квантовая физика"
lokb search "квантовая физика" --format json --mode deep

# Чтение документа
lokb read wikipedia-ru:Quantum_computing

# Поиск сущности
lokb entity "Альберт Эйнштейн" --relations --documents

# Запустить HTTP сервер
lokb serve --port 7890

# Запустить MCP сервер (для Claude Desktop)
lokb serve --mcp
```

## Архитектура

Четырёхслойное хранение по [ADR-001](docs/architecture/adr/001-four-layer-storage.md):

```
RAW SOURCE → OPTIMIZED SOURCE → DERIVED → CACHE
(ZIM, PDF)   (zstd bundles)     (FTS, vectors, graph)  (rendered docs)
```

Параллельный pipeline загрузки ([#46](https://github.com/meteora-pro/lokb/issues/46)):

```
[ZIM Reader]  ──channel──>  [Worker Pool (N)]  ──channel──>  [Writer]
   1 поток                   N потоков                        1 поток
   I/O: декомпрессия         CPU: HTML→MD, chunking           I/O: FTS, SQLite
```

### Структура crates

| Crate | Назначение |
|-------|-----------|
| `lokb-core` | Типы, трейты, конфигурация |
| `lokb-pipeline` | Фреймворк pipeline (OptimizeStep, EnrichmentStep) |
| `lokb-storage` | SQLite каталог, файловое хранилище |
| `lokb-parsers` | Парсеры форматов (ZIM, EPUB, PDF, Telegram, GPX, ...) |
| `lokb-search` | Tantivy FTS, FM-индекс, векторный поиск |
| `lokb-ingest` | Семантический чанкер |
| `lokb-embed` | fastembed ONNX embeddings |
| `lokb-llm` | LLM бэкенды (Ollama) |
| `lokb-graph` | Разрешение сущностей |
| `lokb-render` | TUI просмотр документов (ratatui) |
| `lokb-serve` | HTTP API (axum) + MCP сервер |
| `lokb-tools` | Общие функции для CLI/MCP/HTTP |
| `lokb-cli` | CLI (clap) + параллельный ZIM pipeline |

## Команды CLI

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

## Поддерживаемые форматы

| Формат | Флаг | Описание |
|--------|------|----------|
| `zim` | `--format zim` | Офлайн-дампы Wikipedia/Kiwix |
| `markdown-dir` | `--format markdown-dir` | Директория .md файлов |
| `plaintext-dir` | `--format plaintext-dir` | Директория .txt файлов |
| `html-dir` | `--format html-dir` | Директория .html файлов |
| `epub` | `--format epub` | Электронные книги EPUB |
| `pdf-dir` | `--format pdf-dir` | Директория PDF файлов |
| `telegram-export` | `--format telegram-export` | JSON экспорт Telegram |
| `mbox` | `--format mbox` | Архивы электронной почты MBOX |
| `wikidata-json` | `--format wikidata-json` | JSON дамп Wikidata |
| `gpx` | `--format gpx` | GPS трек-файлы |
| `exif-dir` | `--format exif-dir` | EXIF метаданные фотографий |
| `csv` / `tsv` | `--format csv` | Файлы данных CSV/TSV |
| `chrome-history` | `--format chrome-history` | История браузера Chrome |
| `firefox-history` | `--format firefox-history` | История браузера Firefox |

## Бенчмарки

Тестирование на Apple M3 Max (16 ядер, 48 ГБ RAM):

| Датасет | Размер | Статей | Скорость | Время |
|---------|--------|--------|----------|-------|
| Wiktionary RU | 1.6 ГБ ZIM | 1 456 534 | 880/с | 27 мин |
| Wikisource RU | 4.3 ГБ ZIM | 200 000+ | 600/с | 5 мин |
| Задержка FTS поиска | — | — | — | 26-63 мс |

## Конфигурация

Директория данных: `~/.local/share/lokb/` (переопределяется через `LOKB_DATA_DIR`)

Переменные окружения:

| Переменная | По умолчанию | Описание |
|------------|-------------|----------|
| `LOKB_DATA_DIR` | `~/.local/share/lokb` | Директория данных |
| `LOKB_THREADS` | `num_cpus/2` | Потоки для параллельной загрузки |
| `LOKB_ZIM_BLOCK` | `20000` | Статей на блок коммита |

## Документация

- [Architecture Decision Records](docs/architecture/adr/) — 8 ADR по хранению, pipeline, модели данных
- [Техническая спецификация](docs/SPEC.md) — полный проектный документ
- [Онлайн-документация](https://meteora-pro.github.io/lokb/) — сайт на Rspress

## Разработка

```bash
cargo build                           # сборка workspace
cargo test                            # все тесты
cargo test --test e2e -p lokb-cli     # E2E Gherkin-тесты
cargo clippy --workspace              # линтер
cargo fmt --all                       # форматирование
cargo run -p lokb-cli -- <cmd>        # запуск CLI
```

## Лицензия

Apache 2.0
