# План тестирования lokb — Local Offline Knowledge Base

**Версия:** 1.0
**Дата:** 2026-03-14
**Основа:** README.md v4.0

---

## 1. Введение

Этот документ описывает стратегию тестирования для проекта lokb — персональной offline библиотеки знаний. План основан на техническом задании (README.md) и покрывает все основные компоненты системы от Phase 0 до Phase 6.

### 1.1 Цели тестирования

- Гарантировать корректность работы всех слоёв архитектуры (RAW, OPTIMIZED, DERIVED, CACHE)
- Обеспечить выполнение нефункциональных требований (latency, throughput, storage)
- Проверить корректность pipeline обработки данных
- Валидировать privacy и source transparency
- Убедиться в устойчивости системы к различным форматам данных

### 1.2 Области тестирования

```
┌─────────────────────────────────────────────────────────┐
│ 1. Unit Tests        ← типы, парсеры, трансформации    │
│ 2. Integration Tests ← pipeline, storage, search        │
│ 3. E2E Tests         ← CLI workflows, scenarios         │
│ 4. Performance Tests ← latency, throughput, storage     │
│ 5. Privacy Tests     ← изоляция данных, фильтрация      │
│ 6. Data Quality      ← форматы, корректность извлечения │
└─────────────────────────────────────────────────────────┘
```

---

## 2. Тестирование по фазам разработки

### Phase 0 — Skeleton

#### Тесты для Phase 0

**Unit Tests:**
- [ ] `lokb-core::DataSource` — сериализация/десериализация TOML
- [ ] `lokb-core::DataSourceClass` — правила Public vs Personal
- [ ] `lokb-core::Document` — иерархия (parent_id, depth)
- [ ] `lokb-core::Chunk` — структура данных
- [ ] `lokb-core::Entity` и `Relation` — структура данных
- [ ] `lokb-core::PrivacyLevel` — enum и policy
- [ ] `lokb-core::BudgetManager` — limit tracking
- [ ] `lokb-core::RawRetention` — стратегии хранения

**Integration Tests:**
- [ ] Config parsing — валидный TOML → Config struct
- [ ] Config parsing — невалидный TOML → error messages
- [ ] Filesystem init — создание raw/source/derived/cache
- [ ] Filesystem init — проверка прав доступа
- [ ] BudgetManager — отслеживание лимитов по слоям
- [ ] BudgetManager — degradation при превышении

**Критерии успеха:**
- Все core типы корректно сериализуются
- Filesystem layout создаётся без ошибок
- BudgetManager корректно отслеживает размеры

---

### Phase 1 — Pipeline + Read + FTS

#### Unit Tests

**Pipeline Framework:**
- [ ] `PipelineStep` trait — execute() для mock step
- [ ] `PipelineStep` trait — estimate_cost()
- [ ] `StepFallback` — поведение Skip, SkipDocument, Abort, Retry
- [ ] `PipelineExecutor` — последовательное выполнение шагов
- [ ] `PipelineExecutor` — обработка ошибок
- [ ] `DocumentFilter` — фильтрация документов для шагов

**Extractors:**
- [ ] `MarkdownPassthrough` — валидный markdown → unchanged
- [ ] `PlainTextExtractor` — .txt file → Document
- [ ] `HtmlToMarkdown` — simple HTML → clean markdown
- [ ] `HtmlToMarkdown` — HTML with tables/lists → markdown
- [ ] `HtmlToMarkdown` — malformed HTML → best effort
- [ ] `TextCleaner` — whitespace normalization
- [ ] `TextCleaner` — boilerplate removal
- [ ] `LanguageDetector` — English text → "en"
- [ ] `LanguageDetector` — Russian text → "ru"
- [ ] `LanguageDetector` — Mixed text → primary language

**Content Store:**
- [ ] `ContentStoreWriter` — single document → bundle
- [ ] `ContentStoreWriter` — 1000+ docs → multiple bundles
- [ ] `ContentStoreWriter` — zstd compression ratio check
- [ ] `ContentStoreReader` — read single doc from bundle
- [ ] `ContentStoreReader` — read from cache (second access)
- [ ] `ContentStoreReader` — doc_id → (bundle_id, offset, len)
- [ ] Bundle grouping — same DataSource → same bundle
- [ ] Bundle grouping — same content_type → same bundle

**Chunker:**
- [ ] Semantic chunker — markdown with headers → chunks per section
- [ ] Semantic chunker — plain text → paragraph chunks
- [ ] Semantic chunker — preserve section_path
- [ ] Semantic chunker — byte_start/byte_end accuracy
- [ ] Chunk overlap — последний chunk не слишком короткий
- [ ] Chunk size limits — max 512 tokens/chunk

**FTS:**
- [ ] Tantivy indexer — index 1000 chunks
- [ ] Tantivy indexer — incremental updates
- [ ] BM25 search — single keyword → relevant chunks
- [ ] BM25 search — phrase query → exact matches
- [ ] BM25 search — Boolean operators (AND, OR, NOT)
- [ ] BM25 search — filter by source_id
- [ ] BM25 search — filter by language
- [ ] BM25 search — filter by privacy_level
- [ ] Search latency — <50ms для 50M chunks (mocked)

**Catalog:**
- [ ] SQLite catalog — insert document
- [ ] SQLite catalog — query by document_id
- [ ] SQLite catalog — query by source_id
- [ ] SQLite catalog — query by external_id
- [ ] SQLite catalog — hierarchical queries (parent/children)

**Source Viewer:**
- [ ] Terminal renderer — render plain text document
- [ ] Terminal renderer — render markdown with formatting
- [ ] Terminal renderer — highlight search fragment
- [ ] Terminal renderer — pagination для длинных документов

**CLI:**
- [ ] `lokb source add` — markdown directory
- [ ] `lokb source add` — валидация параметров
- [ ] `lokb source list` — показать все sources
- [ ] `lokb source status <id>` — статус источника
- [ ] `lokb search <query>` — keyword search
- [ ] `lokb search <query> --source <id>` — фильтр по источнику
- [ ] `lokb read <doc_id>` — показать документ
- [ ] `lokb storage status` — показать использование

**RAW Management:**
- [ ] RAW retention — DeleteAfterOptimize удаляет файл
- [ ] RAW retention — Keep сохраняет файл
- [ ] RAW retention — ExternalReference не копирует
- [ ] ReacquireStrategy — Download validation
- [ ] Budget tracking — per-datasource limits

**Skills:**
- [ ] `lokb lookup <fact>` — FTS-based fact lookup
- [ ] `lokb define <term>` — FTS-based definition
- [ ] Skill config parsing — TOML → SkillConfig
- [ ] Custom skill — user-defined skill works

#### Integration Tests

- [ ] **End-to-end markdown ingestion:**
  1. Add markdown directory as DataSource
  2. Optimize (passthrough)
  3. Ingest (chunk + FTS)
  4. Search by keyword
  5. Read found document
  6. Verify citation includes file path

- [ ] **Multi-format pipeline:**
  1. Add mixed directory (markdown + txt + html)
  2. Pipeline routes to correct extractors
  3. All files indexed correctly
  4. Search works across formats

- [ ] **Budget enforcement:**
  1. Set low derived_limit
  2. Ingest large corpus
  3. Verify budget manager stops at limit
  4. Verify error message

- [ ] **Incremental ingestion:**
  1. Ingest 100 documents
  2. Add 10 more documents
  3. Verify only new docs processed
  4. Verify search includes all 110

#### Performance Tests

- [ ] Content Store read latency (cache miss) <500ms
- [ ] Content Store read latency (cache hit) <100ms
- [ ] FTS indexing throughput >10K docs/s
- [ ] FTS search latency <50ms (100K chunks)
- [ ] Zstd compression ratio >5x для text

#### Data Quality Tests

- [ ] Markdown extraction preserves structure
- [ ] HTML→Markdown preserves links
- [ ] HTML→Markdown converts tables correctly
- [ ] Chunk boundary не разрывает предложения
- [ ] Metadata extraction complete (title, language, timestamps)

---

### Phase 2 — Wikipedia + Embeddings

#### Unit Tests

**Wikipedia Extractors:**
- [ ] `ZimArticleExtractor` — extract single article
- [ ] `ZimArticleExtractor` — article with infobox → clean markdown
- [ ] `ZimArticleExtractor` — article with references → preserved
- [ ] `ZimArticleExtractor` — redirect article → resolved
- [ ] `ZimArticleExtractor` — article with images → skip images
- [ ] `WikiXmlOptimizer` — parse XML dump
- [ ] `WikiXmlOptimizer` — wikitext → markdown

**PDF & EPUB:**
- [ ] `PdfOptimizer` — PDF with text layer → markdown
- [ ] `PdfOptimizer` — preserve page numbers
- [ ] `PdfOptimizer` — multi-column layout → linearized
- [ ] `EpubOptimizer` — EPUB chapters → markdown sections
- [ ] `EpubOptimizer` — preserve chapter hierarchy

**Embedding:**
- [ ] Embedding model load (ONNX) — multilingual-e5-small
- [ ] Embedding model inference — single chunk → vector[384]
- [ ] Embedding model inference — batch inference (100 chunks)
- [ ] Embedding throughput CPU >500 chunks/s
- [ ] Vector normalization — L2 norm = 1.0
- [ ] LanceDB vector storage — insert 10K vectors
- [ ] LanceDB vector storage — IVF-PQ index build

**Search:**
- [ ] Vector search — query → top-k chunks
- [ ] Vector search latency <200ms (1M vectors)
- [ ] Hybrid search — RRF fusion
- [ ] Hybrid search — hybrid_alpha = 0.0 → pure FTS
- [ ] Hybrid search — hybrid_alpha = 1.0 → pure vector
- [ ] Hybrid search — hybrid_alpha = 0.7 → balanced
- [ ] SearchMode Quick — FTS only, <50ms
- [ ] SearchMode Normal — Hybrid, <300ms
- [ ] SearchMode Deep — graph expansion (will test in Phase 3)

**Vector Compression:**
- [ ] PQ compression — 384 float32 → 48 bytes
- [ ] PQ compression — recall@10 >0.95
- [ ] Matryoshka dimensions — 384→256→128 degradation
- [ ] Budget-aware degradation — при превышении limit

**HTTP Source Viewer:**
- [ ] Axum server start на localhost:7890
- [ ] HTML rendering документа
- [ ] Highlight search fragments в HTML
- [ ] Browser view показывает citations

**Progressive Loading:**
- [ ] Popular articles indexed first
- [ ] Popularity scoring — pageviews/links
- [ ] Background indexer — incremental progress

#### Integration Tests

- [ ] **Wikipedia ingestion:**
  1. Download mini Wikipedia ZIM (test fixture ~100MB)
  2. Optimize ZIM → markdown bundles
  3. Verify RAW ZIM deleted after optimize
  4. Ingest → chunks + FTS + vectors
  5. FTS search "quantum computing" → results
  6. Vector search "quantum mechanics" → results
  7. Hybrid search — combined results
  8. Verify citations include Wikipedia URL

- [ ] **PDF books ingestion:**
  1. Add directory with 3 PDFs
  2. Optimize → markdown
  3. Verify RAW PDFs kept (retention: Keep)
  4. Search by content
  5. Verify page numbers in citations

- [ ] **Export/Import portable KB:**
  1. Create KB with Wikipedia + Books
  2. Export to .tar.zst
  3. Import to clean instance
  4. Verify search works
  5. Verify only public data exported

#### Performance Tests

- [ ] Wikipedia optimization >10K articles/s
- [ ] Embedding inference CPU >500 chunks/s
- [ ] Vector search latency <200ms (10M vectors)
- [ ] Hybrid search latency <300ms
- [ ] Storage with vectors — Wikipedia 4GB opt + 20GB derived ≈ 24GB

---

### Phase 3 — Knowledge Graph

#### Unit Tests

**Wikidata:**
- [ ] `WikidataOptimizer` — parse JSON entity
- [ ] `WikidataOptimizer` — filter by languages [en, ru]
- [ ] `WikidataOptimizer` — filter by predicates (P31, P279, ...)
- [ ] `WikidataOptimizer` — extract entity labels
- [ ] `WikidataOptimizer` — extract coordinates
- [ ] Entity compression — JSON 90GB → 2.5GB

**RDF:**
- [ ] `RdfParser` — parse N-Triples
- [ ] `RdfParser` — parse Turtle
- [ ] `RdfParser` — oxttl integration
- [ ] Relation extraction — subject/predicate/object

**Entity Resolution:**
- [ ] Entity linking — Wikipedia↔Wikidata by ID
- [ ] Entity linking — by title normalization
- [ ] Entity linking — by embedding similarity
- [ ] Entity linking — geo match (lat/lon)

**Graph Queries:**
- [ ] Entity lookup by ID — <10ms
- [ ] Entity lookup by name — fuzzy match
- [ ] Relation traversal — 1-hop
- [ ] Relation traversal — 2-hop with path
- [ ] Graph expansion в Deep search
- [ ] Semantic graph edges — embedding proximity

**Fact Lookup:**
- [ ] Structured fact from graph — "population of Paris" → 2.1M
- [ ] Fact с citation — [Wikidata Q90, P1082]
- [ ] Fallback to text search если нет в graph

#### Integration Tests

- [ ] **Wikipedia + Wikidata linking:**
  1. Ingest Wikipedia subset
  2. Ingest Wikidata subset
  3. Link entities by external_ids
  4. Query entity "Paris" → shows Wikipedia article + Wikidata facts
  5. Relations → "capital of France", "coordinates", etc.

- [ ] **Graph navigation:**
  1. `lokb entity Paris`
  2. `lokb entity Paris --relations`
  3. `lokb entity Paris --depth 2` → France, Seine, etc.
  4. `lokb entity Paris --documents` → related Wikipedia articles

- [ ] **Fact lookup skill:**
  1. `lokb lookup "population of Tokyo"`
  2. Verify structured answer from Wikidata
  3. Verify citation
  4. Test fallback to text search for obscure fact

#### Performance Tests

- [ ] Entity lookup <10ms
- [ ] Relation traversal <50ms
- [ ] Graph expansion <100ms
- [ ] Wikidata optimization time (90GB → 2.5GB) — benchmark

---

### Phase 4 — Personal Data + Takeout

#### Unit Tests

**Telegram:**
- [ ] `TelegramParser` — parse JSON export
- [ ] `TelegramParser` — extract messages
- [ ] `TelegramParser` — extract media metadata
- [ ] `TelegramParser` — handle edited messages
- [ ] `TelegramParser` — handle forwarded messages
- [ ] `TelegramParser` — parse topics (Telegram groups)
- [ ] `ChatSegmenter` — reply chains → threads
- [ ] `ChatSegmenter` — time gaps >2h → segments
- [ ] `ChatSegmenter` — thread windowing (10 msgs, step 5)
- [ ] Chunk format — conversation context preserved

**Email:**
- [ ] `EmailParser` — parse MBOX
- [ ] `EmailParser` — parse EML
- [ ] `EmailThreader` — In-Reply-To threading
- [ ] `EmailThreader` — References header
- [ ] Thread hierarchy — nested replies

**Photos:**
- [ ] `ExifExtractor` — extract GPS
- [ ] `ExifExtractor` — extract timestamp
- [ ] `ExifExtractor` — extract camera model
- [ ] `ExifExtractor` — no EXIF → fallback to filename
- [ ] `GooglePhotosOptimizer` — parse metadata JSON

**GPS:**
- [ ] `GpxExtractor` — parse GPX
- [ ] `GpxExtractor` — Douglas-Peucker simplification
- [ ] `TrackSegmenter` — day segments
- [ ] `GoogleLocationOptimizer` — Timeline JSON → GeoPoints

**Takeout Dispatcher:**
- [ ] `TakeoutDispatcher` — detect Google Takeout structure
- [ ] `TakeoutDispatcher` — route Gmail → EmailOptimizer
- [ ] `TakeoutDispatcher` — route Photos → PhotoOptimizer
- [ ] `TakeoutDispatcher` — route Location → GpsOptimizer
- [ ] `TakeoutDispatcher` — include/exclude filters
- [ ] Apple Takeout dispatcher
- [ ] Meta Takeout dispatcher

**Privacy:**
- [ ] PrivacyLevel filtering — search with --public-only
- [ ] PrivacyLevel filtering — search with --personal-only
- [ ] PrivacyLevel filtering — Secret не показывается без --include-secret
- [ ] Entity linking — Personal → Public asymmetric
- [ ] Entity linking — Public НЕ знает о Personal
- [ ] Export — personal data excluded по умолчанию
- [ ] Export — personal data included with --include-personal

**File Watch:**
- [ ] File watch — detect new file в Obsidian vault
- [ ] File watch — detect modification
- [ ] File watch — debounce 500ms
- [ ] File watch — incremental re-index

**Geo & Timeline Search:**
- [ ] Geo search — nearby(lat, lon, radius)
- [ ] Geo search — R-tree spatial index
- [ ] Timeline search — range query by timestamp
- [ ] Timeline search — BTree index on timestamp
- [ ] Spatio-temporal linking — photos ↔ GPS ↔ chats

#### Integration Tests

- [ ] **Google Takeout ingestion:**
  1. Prepare test Google Takeout archive
  2. `lokb takeout import ~/takeout.zip --platform google`
  3. Verify created DataSources: gmail, photos, location, bookmarks
  4. Search personal emails
  5. Search photos by location
  6. Timeline view — events по датам
  7. Verify Privacy Level = Private для email

- [ ] **Telegram import:**
  1. Add Telegram export JSON
  2. Parse conversations
  3. Segment threads
  4. Search by message content
  5. Verify conversation context в chunks
  6. Verify Privacy Level = Private

- [ ] **Cross-source linking:**
  1. Import photos (with GPS)
  2. Import GPS tracks
  3. Import Telegram (with location mentions)
  4. Spatio-temporal linking — photo taken в Париже ↔ GPS track ↔ chat "в Париже"
  5. Verify graph connections

- [ ] **Obsidian vault watch:**
  1. Add Obsidian vault as DataSource with --watch
  2. Create new note
  3. Wait for file watch trigger
  4. Verify note indexed
  5. Modify note
  6. Verify re-indexed

- [ ] **Privacy export:**
  1. Create KB with Wikipedia + Telegram
  2. Export without --include-personal
  3. Verify Telegram data not in export
  4. Export with --include-personal
  5. Verify Telegram data included

#### Performance Tests

- [ ] Telegram optimization — 10K messages processing time
- [ ] Email threading — 50K emails
- [ ] EXIF extraction — 10K photos throughput
- [ ] Geo search latency <100ms
- [ ] Timeline search latency <50ms

#### Privacy Tests

- [ ] **Isolation test:**
  1. Create Public source (Wikipedia)
  2. Create Personal source (Telegram)
  3. Telegram mentions "Paris" → links to Entity:Paris
  4. Entity:Paris query → does NOT show Telegram mentions
  5. Verify one-way linking

- [ ] **Filter test:**
  1. Mixed corpus (public + personal)
  2. Search --public-only → only Wikipedia
  3. Search --personal-only → only Telegram
  4. Search without filter → both (with privacy indicators)

- [ ] **Export test:**
  1. Export public KB
  2. Verify no PII in export
  3. Verify no personal DataSource configs
  4. Import exported KB → verify usable

---

### Phase 5 — LLM Integration

#### Unit Tests

**LLM Backend:**
- [ ] `OllamaBackend` — model availability check
- [ ] `OllamaBackend` — generate request
- [ ] `OnnxBackend` — model load
- [ ] `CandleBackend` — model load
- [ ] `OpenAiCompatibleBackend` — API call
- [ ] `SkipBackend` — fallback behavior

**Enrichers:**
- [ ] `ImageDescriber` — photo → description via vision LLM
- [ ] `ImageDescriber` — fallback=skip при отсутствии LLM
- [ ] `SpeechToText` — audio → text via Whisper
- [ ] `SpeechToText` — Whisper model loading
- [ ] `VideoDescriber` — keyframes extraction
- [ ] `VideoDescriber` — keyframes → description via vision LLM
- [ ] `TextSummarizer` — long text → summary
- [ ] Pipeline config — LLM step enabled/disabled
- [ ] Pipeline config — LLM model selection

**MCP Server:**
- [ ] MCP server start — `lokb serve --mcp`
- [ ] MCP protocol — tool listing
- [ ] MCP protocol — search tool call
- [ ] MCP protocol — read tool call
- [ ] Citation в LLM response — verify format

**Enrichment CLI:**
- [ ] `lokb enrich <source> --step image_describer`
- [ ] `lokb enrich` — run on existing data
- [ ] `lokb enrich` — progress tracking
- [ ] `lokb enrich` — resume after interruption

#### Integration Tests

- [ ] **Photo enrichment:**
  1. Ingest photos (EXIF only)
  2. `lokb enrich photos --step image_describer --llm ollama:llava`
  3. Verify descriptions added to optimized source
  4. Search by description content
  5. Verify citation includes EXIF + description

- [ ] **Telegram voice messages:**
  1. Telegram export with voice messages
  2. Pipeline with SpeechToText step
  3. Verify transcripts in chunks
  4. Search by voice content

- [ ] **Video processing:**
  1. Add video directory
  2. Pipeline: metadata + keyframe description + STT
  3. Verify video searchable by spoken content
  4. Verify video searchable by visual description

- [ ] **MCP integration:**
  1. Start `lokb serve --mcp`
  2. Connect LLM client (mock)
  3. LLM asks "population of Paris"
  4. Verify structured response with citation
  5. Verify LLM can read full documents

#### Performance Tests

- [ ] STT throughput — Whisper speed
- [ ] Image description — LLaVA latency
- [ ] Enrichment batch processing — parallelization

---

### Phase 6 — Polish

#### Unit Tests

**Incremental Sync:**
- [ ] Change detection — modified files
- [ ] Change detection — deleted files
- [ ] Change detection — new files
- [ ] Delta update — minimize re-processing

**Additional Optimizers:**
- [ ] `ArxivOptimizer` — parse arXiv JSONL
- [ ] `StackOverflowOptimizer` — Q&A format

**Compaction:**
- [ ] Bundle compaction — merge small bundles
- [ ] Index compaction — remove deleted docs
- [ ] Cache eviction — LRU

**Interactive Annotation:**
- [ ] Interactive annotation mode
- [ ] TUI для annotation workflow
- [ ] Annotation persistence

#### Integration Tests

- [ ] **Incremental Wikipedia update:**
  1. Ingest Wikipedia 2024-01
  2. Update to Wikipedia 2024-03
  3. Verify only changed articles re-processed
  4. Verify deleted articles removed
  5. Verify new articles added

- [ ] **Storage compaction:**
  1. Create KB, delete some sources
  2. `lokb storage compact`
  3. Verify space reclaimed
  4. Verify data integrity

#### Performance Tests

- [ ] Startup latency <2s
- [ ] Memory usage <500MB RSS
- [ ] Binary size <30MB (without model)

---

## 3. Нефункциональные требования — тестирование

### 3.1 Latency Tests

| Операция | Target | Тест |
|---|---|---|
| Document read (cache hit) | <100ms | Прочитать документ дважды, второй раз <100ms |
| Document read (cache miss) | <500ms | Прочитать непрочитанный документ <500ms |
| FTS search (50M chunks) | <50ms | Mock 50M chunks, query <50ms |
| Vector search (50M chunks) | <200ms | Mock 50M vectors, query <200ms |
| Hybrid search | <300ms | Combined query <300ms |
| Entity lookup | <10ms | Query entity by ID <10ms |

### 3.2 Throughput Tests

| Операция | Target | Тест |
|---|---|---|
| Optimize (PDF) | ~1K pages/s | Benchmark 100-page PDF |
| Optimize (Wiki articles) | ~10K articles/s | Benchmark Wikipedia subset |
| Chunk + FTS index | >10K docs/s | Benchmark ingestion pipeline |
| Embedding (CPU) | >500 chunks/s | Benchmark embedding on CPU |
| Embedding (GPU) | >5K chunks/s | Benchmark embedding on GPU (if available) |

### 3.3 Storage Tests

**Типичная конфигурация:**
- [ ] Wikipedia EN — 4GB optimized + 20GB derived ≈ 24GB
- [ ] Wikidata — 2.5GB optimized + 3GB derived ≈ 5.5GB
- [ ] 1000 Books — 2GB optimized + 8GB derived ≈ 10GB (raw kept)
- [ ] Total (no raw) — ~10GB optimized + ~36GB derived ≈ ~55GB
- [ ] With PQ vectors — ~10GB optimized + ~16GB derived ≈ ~35GB

**Compression ratio tests:**
- [ ] Zstd text compression >5x
- [ ] Bundle clustering +20-40% boost
- [ ] PQ vector compression 30x (384→48 bytes)

### 3.4 System Tests

- [ ] Startup time <2s
- [ ] Memory footprint <500MB RSS
- [ ] Binary size <30MB (без модели)
- [ ] Binary size <150MB (с bundled ONNX model)
- [ ] Platforms: macOS (arm64, x86_64), Linux (x86_64, arm64), Windows

### 3.5 Portability Test

- [ ] Копирование `~/.local/share/lokb/` на другую машину
- [ ] Verify search работает
- [ ] Verify read работает
- [ ] Verify integrity после переноса

---

## 4. Специальные виды тестирования

### 4.1 Data Quality Tests

**Extraction Quality:**
- [ ] HTML→Markdown preserves structure (headers, lists, tables)
- [ ] HTML→Markdown preserves links
- [ ] PDF→Markdown preserves page numbers
- [ ] PDF→Markdown handles multi-column layouts
- [ ] Wikipedia→Markdown removes navigation/boilerplate
- [ ] EXIF extraction complete (GPS, timestamp, camera)
- [ ] STT transcription accuracy (WER <10% на clean audio)
- [ ] Image description relevance (manual eval)

**Chunking Quality:**
- [ ] Chunk boundaries не разрывают предложения
- [ ] Semantic boundaries respected (headers)
- [ ] section_path корректно отслеживается
- [ ] byte_start/byte_end точность

**Citation Quality:**
- [ ] Каждый результат имеет citation
- [ ] Citation contains display string
- [ ] Citation contains local_view link
- [ ] Citation contains web_url (для public)
- [ ] Citation respects privacy level

### 4.2 Robustness Tests

**Malformed Input:**
- [ ] Malformed HTML → best-effort extraction
- [ ] Malformed PDF → graceful error
- [ ] Truncated JSON → error handling
- [ ] Invalid UTF-8 → replacement chars
- [ ] Empty files → skip gracefully
- [ ] Huge files (>1GB) → streaming processing

**Edge Cases:**
- [ ] Document без title → fallback to filename
- [ ] Document без language → LanguageDetector
- [ ] Chunk слишком большой (>10K tokens) → split further
- [ ] Chunk слишком маленький (<10 tokens) → merge with previous
- [ ] Entity без coordinates → skip geo linking
- [ ] Relation без subject/object → validation error

**Error Recovery:**
- [ ] Pipeline step failure → fallback behavior
- [ ] LLM unavailable → skip enrichment
- [ ] Disk full → graceful degradation
- [ ] Corrupted bundle → error + recovery suggestion
- [ ] Index corruption → rebuild from source

### 4.3 Privacy & Security Tests

**Privacy Isolation:**
- [ ] Public source не видит Personal mentions
- [ ] Personal source может ссылаться на Public entities
- [ ] Search results показывают privacy level
- [ ] Export исключает Personal по умолчанию
- [ ] Web view не показывает Private без authentication (future)

**Data Safety:**
- [ ] Secrets detection — .env, credentials.json не индексируются
- [ ] PII warning — при import email/chats
- [ ] Encryption (future) — personal data at rest

**Access Control:**
- [ ] PrivacyLevel enforced в search
- [ ] Secret level требует explicit --include-secret
- [ ] File path не leak в web URLs

### 4.4 Compatibility Tests

**Formats:**
- [ ] Wikipedia ZIM — Kiwix format compatibility
- [ ] Wikipedia XML — MediaWiki dump compatibility
- [ ] Wikidata JSON — current format
- [ ] PDF — multiple versions (1.3 - 2.0)
- [ ] EPUB — EPUB2, EPUB3
- [ ] Telegram — current export format
- [ ] Email — MBOX, EML, Maildir
- [ ] GPX — 1.0, 1.1
- [ ] EXIF — multiple image formats (JPG, PNG, HEIC, RAW)

**Platforms:**
- [ ] macOS arm64 — build + run tests
- [ ] macOS x86_64 — build + run tests
- [ ] Linux x86_64 — build + run tests
- [ ] Linux arm64 — build + run tests
- [ ] Windows x86_64 — build + run tests

**Dependencies:**
- [ ] LanceDB version compatibility
- [ ] Tantivy version compatibility
- [ ] ONNX Runtime versions (CPU, GPU)
- [ ] Oxttl/oxrdfio versions

---

## 5. Test Fixtures & Test Data

### 5.1 Минимальные Test Fixtures

**Phase 1:**
- [ ] 100 markdown files (mixed sizes, languages)
- [ ] 50 plain text files
- [ ] 20 HTML files (varied complexity)

**Phase 2:**
- [ ] Mini Wikipedia ZIM (~100MB, ~1000 articles)
- [ ] 5 PDF documents (text, scanned, mixed)
- [ ] 3 EPUB books

**Phase 3:**
- [ ] Wikidata subset (~1000 entities, filtered)
- [ ] N-Triples RDF sample

**Phase 4:**
- [ ] Google Takeout sample (synthetic, anonymized)
- [ ] Telegram export sample (10 conversations)
- [ ] Email MBOX (100 emails, threaded)
- [ ] GPX tracks (5 days)
- [ ] Photos with EXIF (50 images)

**Phase 5:**
- [ ] Audio files for STT (5 samples, clean audio)
- [ ] Images for vision LLM (20 varied scenes)
- [ ] Videos (3 short clips)

### 5.2 Synthetic Data Generation

- [ ] Synthetic markdown generator (controlled size/structure)
- [ ] Synthetic conversation generator (reply chains)
- [ ] Synthetic GPS tracks generator
- [ ] Synthetic entity graph generator

---

## 6. Test Execution Strategy

### 6.1 Continuous Integration

```yaml
# CI Pipeline
stages:
  - unit_tests       # быстрые, на каждый commit
  - integration_tests # средние, на PR
  - e2e_tests        # длинные, на merge to main
  - performance_tests # nightly
  - compatibility_tests # weekly
```

### 6.2 Test Organization

```
lokb/
├── crates/
│   ├── lokb-core/
│   │   ├── src/
│   │   └── tests/
│   │       ├── unit/
│   │       │   ├── datasource_tests.rs
│   │       │   ├── budget_tests.rs
│   │       │   └── ...
│   │       └── integration/
│   │           ├── config_tests.rs
│   │           └── ...
│   ├── lokb-pipeline/
│   │   ├── src/
│   │   └── tests/
│   │       ├── unit/
│   │       │   ├── step_tests.rs
│   │       │   ├── executor_tests.rs
│   │       │   └── ...
│   │       └── integration/
│   │           ├── pipeline_e2e_tests.rs
│   │           └── ...
│   └── ...
├── tests/
│   ├── e2e/
│   │   ├── markdown_workflow_test.rs
│   │   ├── wikipedia_workflow_test.rs
│   │   ├── personal_data_workflow_test.rs
│   │   └── ...
│   ├── performance/
│   │   ├── latency_benchmarks.rs
│   │   ├── throughput_benchmarks.rs
│   │   └── storage_benchmarks.rs
│   ├── privacy/
│   │   ├── isolation_tests.rs
│   │   ├── export_tests.rs
│   │   └── ...
│   └── fixtures/
│       ├── markdown/
│       ├── wikipedia-mini.zim
│       ├── wikidata-sample.json
│       └── ...
```

### 6.3 Test Commands

```bash
# Unit tests (быстрые)
cargo test --lib

# Integration tests
cargo test --test '*'

# E2E tests
cargo test --test e2e

# Performance benchmarks
cargo bench

# Specific crate tests
cargo test -p lokb-pipeline

# With coverage
cargo tarpaulin --out Html

# Specific phase tests
cargo test --test phase1_tests
cargo test --test phase2_tests
```

### 6.4 Test Coverage Goals

| Phase | Target Coverage |
|---|---|
| Phase 0 | >90% (core types, simple logic) |
| Phase 1 | >80% (pipeline, storage) |
| Phase 2 | >75% (complex extractors) |
| Phase 3 | >75% (graph logic) |
| Phase 4 | >70% (many parsers) |
| Phase 5 | >65% (LLM integration, harder to mock) |
| Phase 6 | >70% (polish) |

---

## 7. Особые сценарии тестирования

### 7.1 Regression Tests

После каждой фазы создаём snapshot tests:
- [ ] Known query → expected results (exact match)
- [ ] Known document → expected chunks
- [ ] Known entity → expected relations

### 7.2 Upgrade Tests

- [ ] V1 data → V2 format migration
- [ ] Schema evolution compatibility
- [ ] Index rebuild при breaking changes

### 7.3 Stress Tests

- [ ] 100M chunks ingestion
- [ ] 1B vectors (с PQ)
- [ ] 10K concurrent searches
- [ ] 24h continuous operation
- [ ] Memory leak detection (valgrind)

### 7.4 User Acceptance Tests

**Scenarios:**
- [ ] "Я хочу найти информацию о квантовых компьютерах"
  - Search "quantum computing"
  - Verify relevant Wikipedia articles
  - Verify citations present
  - Read article via viewer

- [ ] "Я хочу добавить свои книги"
  - `lokb source add books --raw ~/Books --format pdf-dir`
  - Wait for optimization
  - Search by book content
  - Verify PDF file paths in citations

- [ ] "Я хочу импортировать Google Takeout"
  - `lokb takeout import ~/takeout.zip --platform google`
  - Verify multiple sources created
  - Search personal emails
  - Verify privacy levels

- [ ] "Я хочу найти где я обсуждал ресторан в чатах"
  - `lokb personal "ресторан" --after 2024-01-01`
  - Verify conversation context
  - Verify thread view

- [ ] "Я хочу посмотреть что я делал в Париже"
  - `lokb nearby 48.858,2.294 --radius 5km`
  - Verify photos, GPS tracks, chats cross-linked
  - Verify timeline view

---

## 8. Метрики качества

### 8.1 Code Quality Metrics

- [ ] Clippy warnings = 0
- [ ] `cargo fmt` соответствие
- [ ] Документация pub functions >80%
- [ ] No `unwrap()` в production коде (только `expect()` с пояснением)
- [ ] Error handling — все `Result<T, E>` обрабатываются

### 8.2 Search Quality Metrics

**Precision/Recall:**
- [ ] FTS precision@10 >0.8 (на known queries)
- [ ] Vector search recall@10 >0.9
- [ ] Hybrid search beats both FTS и vector separately

**Citation Quality:**
- [ ] 100% results имеют citation
- [ ] Citation links работают (local view)
- [ ] Web URLs valid для public sources

### 8.3 Data Integrity Metrics

- [ ] 0% data loss в pipeline
- [ ] Chunk coverage — весь text покрыт chunks
- [ ] Metadata completeness >95%

---

## 9. Инструменты тестирования

### 9.1 Rust Testing Tools

```toml
[dev-dependencies]
# Testing
assert_cmd = "2"        # CLI testing
predicates = "3"        # assertions
tempfile = "3"          # temp directories
proptest = "1"          # property-based testing
quickcheck = "1"        # property testing
mockall = "0.12"        # mocking
fake = "2"              # fake data generation

# Benchmarking
criterion = "0.5"       # micro-benchmarks
divan = "0.1"           # faster alternative

# Coverage
tarpaulin = "0.27"      # coverage reports

# Snapshot testing
insta = "1"             # snapshot testing
```

### 9.2 Performance Profiling

- [ ] `cargo flamegraph` — CPU profiling
- [ ] `heaptrack` — memory profiling
- [ ] `hyperfine` — CLI benchmarking
- [ ] `perf` — Linux profiling

### 9.3 Debugging Tools

- [ ] `tracing` subscriber для tests
- [ ] `RUST_LOG=debug` environment
- [ ] `dbg!()` макросы (удалить перед commit)

---

## 10. Acceptance Criteria

### Phase 1 Ready When:

- [ ] ✅ Markdown/text ingestion works E2E
- [ ] ✅ FTS search returns relevant results
- [ ] ✅ Source viewer shows documents correctly
- [ ] ✅ CLI commands работают
- [ ] ✅ All Phase 1 unit tests pass
- [ ] ✅ All Phase 1 integration tests pass
- [ ] ✅ Test coverage >80%

### Phase 2 Ready When:

- [ ] ✅ Wikipedia ZIM ingestion works
- [ ] ✅ PDF/EPUB ingestion works
- [ ] ✅ Embeddings вычисляются
- [ ] ✅ Hybrid search works
- [ ] ✅ Export/Import works
- [ ] ✅ All Phase 2 tests pass
- [ ] ✅ Performance targets met

### Phase 3 Ready When:

- [ ] ✅ Wikidata ingestion works
- [ ] ✅ Entity linking works
- [ ] ✅ Graph queries work
- [ ] ✅ Fact lookup works
- [ ] ✅ All Phase 3 tests pass

### Phase 4 Ready When:

- [ ] ✅ Takeout import works
- [ ] ✅ Personal data parsers work
- [ ] ✅ Privacy isolation verified
- [ ] ✅ Geo/Timeline search works
- [ ] ✅ All Phase 4 tests pass
- [ ] ✅ Privacy tests pass

### Phase 5 Ready When:

- [ ] ✅ LLM backends работают
- [ ] ✅ Image/Video/Audio enrichment works
- [ ] ✅ MCP server works
- [ ] ✅ All Phase 5 tests pass

### Phase 6 Ready When:

- [ ] ✅ Incremental updates work
- [ ] ✅ Storage compaction works
- [ ] ✅ All tests pass
- [ ] ✅ Performance verified
- [ ] ✅ Documentation complete
- [ ] ✅ Ready for v1.0 release

---

## 11. Приоритизация тестирования

### Высокий приоритет (Critical Path):

1. Core types serialization
2. Pipeline execution
3. Content Store read/write
4. FTS search
5. Citation generation
6. Privacy isolation
7. Budget enforcement

### Средний приоритет:

1. Advanced extractors (PDF, EPUB)
2. Vector search
3. Entity linking
4. Personal data parsers
5. Export/Import

### Низкий приоритет (Nice to Have):

1. LLM enrichment качество
2. Interactive annotation UX
3. HTTP viewer UI/UX
4. Advanced graph queries
5. Storage optimization details

---

## 12. Заключение

Этот план тестирования обеспечивает:

- **Полное покрытие** всех фаз разработки
- **Систематический подход** от unit к E2E tests
- **Нефункциональные требования** (performance, privacy, storage)
- **Data quality** verification
- **Clear acceptance criteria** для каждой фазы

План будет обновляться по мере развития проекта.

---

**Версия:** 1.0
**Дата:** 2026-03-14
**Статус:** Draft
**Следующий шаг:** Review → Implement Phase 0 tests
