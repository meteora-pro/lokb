# lokb — Local Offline Knowledge Base

**Полное техническое задание v4.0**
**Язык:** Rust | **Лицензия:** Apache-2.0 / MIT dual

---

## 1. Миссия

lokb — **персональная offline библиотека знаний**. Компактное хранилище,
объединяющее публичные знания (Wikipedia, Wikidata, книги, научные статьи)
и личные данные (переписки, заметки, фото, GPS, takeout-экспорты)
в единую поисковую систему.

**Для человека** — читать, искать, навигировать по огромному массиву знаний
без интернета.

**Для LLM** — верифицируемый источник фактов с citations.
Модель находит информацию, цитирует источники и не галлюцинирует.

**Embedding search — одна из фич, не цель.**
Цель — эффективное построение offline базы знаний с множеством способов доступа.

### Принципы

| Принцип | Описание |
|---|---|
| **Offline-first** | Всё работает без интернета |
| **CLI-first** | Composable unix tool, pipe-friendly |
| **Data sovereignty** | Данные не покидают машину |
| **Compact storage** | 55GB = Wikipedia + Wikidata + 1000 книг + личное |
| **Ideas, not files** | Всё превращается в текст — хранилище идей |
| **Source transparency** | Каждый результат имеет проверяемую ссылку |
| **Incremental** | Обновление без полной пересборки |
| **Pipeline composability** | Гибкие цепочки обработки с LLM/human шагами |

---

## 2. Четырёхслойная архитектура хранения

### 2.1 Обзор слоёв

```
RAW SOURCE          → оригинал как скачан (PDF, ZIM, JSON dump, JPG, MP4)
  ↓ Composable Pipeline (extractors + LLM enrichers + human annotations)
OPTIMIZED SOURCE    → нормализованный текст, "хранилище идей" (zstd bundles)
  ↓ Ingestion (chunk + index + embed)
DERIVED             → индексы для поиска (chunks, vectors, FTS, graph)
  ↓ On-demand
CACHE               → рендеренные документы, распакованные блоки
```

### 2.2 RAW SOURCE

Файлы в исходном формате. Неоптимальны. **Можно удалить** после оптимизации.

| RAW | Размер | После optimize | Retention |
|---|---|---|---|
| wikipedia.zim | 25-97 GB | 4 GB markdown | Удалить |
| wikidata.json.gz | 90 GB | 2.5 GB filtered | Удалить |
| book.pdf | 15 MB | 800 KB markdown | Хранить |
| IMG_1234.jpg | 8 MB | 2 KB EXIF + 500B description | External |
| video.mp4 | 2 GB | 50 KB metadata+transcript | External |
| telegram export | 500 MB | 50 MB normalized | Удалить |

```rust
enum RawRetention {
    DeleteAfterOptimize,         // дампы — перескачаем
    Keep,                        // книги — может не быть в сети
    ExternalReference(PathBuf),  // фото/видео — остаются на месте
    KeepVersions(u32),           // дампы с обновлениями
}

enum ReacquireStrategy {
    Download { url: String, expected_hash: Option<Blake3Hash> },
    CopyFrom(PathBuf),
    Torrent { magnet: String },
    None,
}
```

**Budget:** 0-200 GB (опционально, может быть на внешнем диске)

### 2.3 OPTIMIZED SOURCE — хранилище идей

Единый текстовый формат. Не "сжатые оригиналы", а **extracted ideas**.
Фото 8MB → текстовое описание 500 bytes. Видео 2GB → transcript 50KB.

**Всё становится текстом**, потому что текст:
- Максимально сжимаем (zstd: 5-10x)
- Индексируется единообразно (FTS, chunks, embeddings)
- Понятен и человеку, и LLM

**Content Store** — cluster-bundle формат (идея из Kiwix ZIM):
- Документы группируются по ~1000 per bundle, по DataSource и content_type
- Bundle сжимается одним zstd блоком (словарь → +20-40% экономия)
- Индекс: `doc_id → (bundle_id, offset, compressed_len, original_len)`
- Для одного документа распаковать только его bundle

**Budget:** 20-50 GB (обязательно, source of truth)

### 2.4 DERIVED — вычислено из OPTIMIZED SOURCE

| Компонент | Назначение | Стоимость пересоздания |
|---|---|---|
| Chunks (LanceDB) | Фрагменты + metadata | Минуты |
| FTS Index (Tantivy) | Keyword search, BM25 | ~30 мин |
| Embedding Vectors (LanceDB) | Semantic search | **Часы CPU / мин GPU** |
| Entity/Relation (LanceDB+SQLite) | Knowledge graph | Минуты (RDF) / часы (NER) |
| Catalog (SQLite) | Метаданные документов | Минуты |

**Ключевое:** Chunk.vector = Optional. FTS и browse доступны **сразу**.
Semantic search подключается по мере вычисления embeddings в фоне.

**Budget:** 30-80 GB (embedding vectors — основной объём)

### 2.5 CACHE — расходный, LRU eviction

Рендеренные HTML, распакованные bundles, query cache.
Пересоздаётся за миллисекунды. **Budget:** 10-20 GB

### 2.6 Граф стоимости пересоздания

```
Дёшево (мс)        Средне (мин)        Дорого (часы)
──────────          ──────────          ──────────
Render HTML         Parse + Chunk       Embeddings
Decompress bundle   FTS index           NER extraction
Query cache         Entity import       LLM enrichment (image desc)
                    Catalog rebuild     Semantic graph edges

← CACHE →           ← DERIVED(cheap) →  ← DERIVED(expensive) →
```

---

## 3. Public vs Personal DataSources

### 3.1 Два мира данных

```rust
enum DataSourceClass {
    Public {
        license: String,
        web_url_template: Option<String>,
        exportable: bool,          // можно в portable KB export
    },
    Personal {
        owner: String,
        platform: Platform,
        contains_pii: bool,
        exportable: bool,          // default: false
    },
}
```

### 3.2 Правила взаимодействия

```
Public ↔ Public:   свободно линкуется
  Wikipedia article ↔ Wikidata entity

Personal → Public: асимметрично
  "мой чат mentions Paris" → link to Entity:Paris
  НО: Entity:Paris НЕ знает о моих чатах
  При export: personal links не включаются

Personal ↔ Personal: внутри privacy boundary
  "фото из Парижа" ↔ "GPS в Париже" ↔ "чат о Париже"
```

### 3.3 Privacy Levels

```rust
enum PrivacyLevel {
    Public = 0,     // Wikipedia → web URL, entity linking, export
    Internal = 1,   // Книги, заметки → file path, entity linking
    Private = 2,    // Чаты, email → только local view
    Secret = 3,     // Не показывать без явного фильтра
}

struct PrivacyPolicy {
    default_level: PrivacyLevel,
    allow_web_url: bool,
    allow_file_path: bool,
    allow_entity_linking: bool,
    allow_graph_expansion: bool,
}
```

---

## 4. DataSource

### 4.1 Типы

```rust
enum DataSourceKind {
    Corpus(CorpusConfig),        // Wikipedia, книги, статьи, документация
    Graph(GraphConfig),          // RDF, Wikidata
    Personal(PersonalConfig),    // чаты, email, заметки
    MediaMeta(MediaMetaConfig),  // EXIF, GPS, видео
    Structured(StructuredConfig),// CSV, SQLite, Parquet
}
```

### 4.2 Поддерживаемые форматы

**Corpus:** Kiwix ZIM, Wikipedia XML, Markdown dir, PlainText dir, HTML dir,
PDF dir, EPUB dir, arXiv JSONL, Custom JSONL

**Graph:** Wikidata JSON, N-Triples/Turtle (oxttl), JSON-LD, ConceptNet TSV

**Personal:** Telegram, WhatsApp, Slack, Discord, Email MBOX/EML,
Obsidian vault, Browser bookmarks, Calendar ICS, voice memos

**MediaMeta:** EXIF (JPG/RAW), GPX, Google Location History, Video metadata

**Structured:** CSV/TSV, JSONL, SQLite, Parquet

**Takeout:** Google Takeout, Apple export, Meta (Facebook/Instagram)

### 4.3 Sync Strategy

```rust
enum SyncStrategy {
    Once,                                        // дампы
    Incremental { change_detection: ChangeDetection },
    FileWatch { debounce: Duration },            // Obsidian, фото
    FullReload,
}
```

### 4.4 DataSource Links

```rust
enum LinkKind {
    Enriches { entity_mapping: EntityMapping },  // Wikidata → Wikipedia
    References,                                   // Bookmarks → Wikipedia
    SpatioTemporal { time_tolerance: Duration, distance_m: f64 },
    SubsetOf,
}

enum EntityMapping {
    ById { source_field: String, target_field: String },
    ByTitle { normalize: bool },
    EmbeddingMatch { threshold: f32 },
    GeoMatch { max_distance_m: f64 },
}
```

---

## 5. Data Model

### 5.1 Document — иерархическая единица

```rust
struct Document {
    id: DocumentId,              // UUID v7
    source_id: DataSourceId,
    external_id: String,

    // Иерархия
    parent_id: Option<DocumentId>,
    depth: u8,                   // 0 = root

    // Контент
    title: String,
    content_type: ContentType,
    language: Option<Language>,
    content_hash: Blake3Hash,
    content_size: u64,

    // Время
    created_at: DateTime<Utc>,
    indexed_at: DateTime<Utc>,

    // Source transparency
    source_ref: SourceRef,

    // Type-dependent metadata
    metadata: DocumentMetadata,

    // Entity links
    entity_links: Vec<EntityLink>,
}

enum ContentType {
    Article, Book, Paper, Note, Webpage, CodeFile,
    Conversation, Thread,
    EmailThread, Email,
    MediaMeta,
    GpsTrackFile, GpsSegment,
    Record,
}
```

**Иерархия:**
```
Book(Root)         → Chapter(Section)     → Chunks
Conversation(Root) → Thread(Section)      → Chunks (message windows)
EmailThread(Root)  → Email(Section)       → Chunks
GpsTrackFile(Root) → DaySegment(Section)  → Chunks
Article(Root)      → Chunks (flat)
```

### 5.2 Chunk — единица поиска

```rust
struct Chunk {
    id: ChunkId,
    document_id: DocumentId,
    source_id: DataSourceId,     // денормализовано
    chunk_index: u32,

    text: String,
    vector: Option<FixedSizeList<f32>>,  // OPTIONAL — вычисляется в фоне

    // Позиция в оригинале
    byte_start: u64,
    byte_end: u64,
    section_path: Option<String>,
    page_number: Option<u32>,

    // Фильтрация
    content_type: String,
    language: Option<String>,
    timestamp: Option<i64>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    privacy_level: u8,
    entity_ids: Option<String>,  // JSON array
}
```

### 5.3 Entity — узел графа

```rust
struct Entity {
    id: EntityId,
    canonical_name: String,
    labels: HashMap<Language, Vec<String>>,
    description: Option<String>,
    types: Vec<EntityType>,
    external_ids: HashMap<String, String>,
    // {"wikidata": "Q90", "wikipedia-en": "Paris", "osm": "node/17807753"}
    name_vector: Option<FixedSizeList<f32>>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    mention_count: u32,
}
```

### 5.4 Relation — ребро графа

```rust
struct Relation {
    subject_id: EntityId,
    predicate: PredicateId,
    object_id: EntityId,
    source_id: DataSourceId,
    confidence: f32,
    qualifiers: Option<HashMap<String, Value>>,
}

enum PredicateKind {
    // Из RDF: Ontological, Relational, Temporal, Spatial
    // Извлечённые: Mentions, CoOccurrence
    // Личные: SentBy, TakenAt, TakenWith, VisitedAt
    // Вычисленные: SemanticEdge (embedding proximity, txtai-style)
    // Custom(String)
}
```

### 5.5 OPTIMIZED Document — хранилище идей

Всё превращается в текст. Каждый документ — набор идей.

```rust
struct OptimizedDocument {
    document: Document,
    text_content: String,        // основной текст (идеи)
    text_format: TextFormat,     // Markdown | PlainText
    source_refs: Vec<SourceRef>, // ссылки на оригиналы
    annotations: Vec<Annotation>,// human + LLM аннотации
}

struct Annotation {
    author: AnnotationAuthor,    // Human | Llm | Pipeline
    text: String,
    anchor: Option<TextSpan>,
    created_at: DateTime<Utc>,
}
```

**Примеры OPTIMIZED для разных типов:**

Фото → `"Sunset at Eiffel Tower from Trocadéro. iPhone 15 Pro. 48.862°N 2.288°E. March 15, 2024."`

Голосовое → `"Voice from Alice (1:23): Hey, just arrived at the hotel in Paris..."`

Видео → `"Family dinner. [0:00] Restaurant interior... [0:45] Toast... Transcript: ..."`

Таблица → `"Table shows quarterly revenue. Q1: $2.3M (+12%), Q2: $2.8M (+22%)..."`

### 5.6 Source Transparency

```rust
struct Citation {
    display: String,             // "Wikipedia: Quantum Computing, §Error Correction"
    local_view: LocalViewLink,   // всегда offline
    web_url: Option<String>,     // fallback для публичных
    file_path: Option<PathBuf>,  // для локальных/медиа
    privacy: PrivacyLevel,
}
```

---

## 6. Messaging Data Model

### 6.1 Сегментация чатов

Одно сообщение ("Да") бессмысленно для поиска. Нужен контекст.

```
1. Явные треды платформы (Slack, Telegram topics) → Thread
2. Reply-chains (A→B→C транзитивно) → Thread
3. Временные разрывы (>2ч тишины) → inferred Segment
4. Внутри Thread/Segment: окно 10 сообщений, шаг 5 → Chunk
```

### 6.2 Поддержка платформ

```
            Threads  Reply-to  Topics  Reactions  Edited  Forwarded
Telegram      ✗        ✓        ✓*       ✓         ✓        ✓
Slack         ✓        ✓**      ✗        ✓         ✓        ✗
Discord       ✓        ✓        ✓***     ✓         ✓        ✗
WhatsApp      ✗        ✓        ✗        ✓         ✓        ✓
Email         ✓****    ✓****    ✗        ✗         ✗        ✓
```

### 6.3 Chunk формат из чата

```
[Rust Dev, 2024-03-15]
Alice [14:20]: Кто-нибудь использовал LanceDB в проде?
Bob [14:21]: Да, у нас. Работает стабильно.
Alice [14:22]: Какой размер базы?
Bob [14:23]: ~5M vectors, 384 dims. На диске ~3GB.
```

---

## 7. Composable Optimization Pipeline

### 7.1 Архитектура

Pipeline RAW→OPTIMIZED — **цепочка шагов**. Каждый шаг — extractor, enricher,
transformer или writer. В цепочку можно вставлять LLM и human annotations.

```
RAW file → [Step 1: Extract] → [Step 2: Enrich via LLM] → [Step 3: Clean]
         → [Step 4: Annotate] → [Step 5: Write to Content Store]
```

### 7.2 PipelineStep trait

```rust
#[async_trait]
trait PipelineStep: Send + Sync {
    fn name(&self) -> &str;
    fn input_type(&self) -> StepDataType;
    fn output_type(&self) -> StepDataType;
    async fn execute(&self, input: StepData, ctx: &PipelineContext) -> Result<StepData>;
    fn estimate_cost(&self, input_size: ByteSize) -> StepCost;
}

struct StepCost {
    estimated_time: Duration,
    needs_gpu: bool,
    needs_llm: bool,
    needs_network: bool,
    needs_human: bool,
}
```

### 7.3 Встроенные шаги

**Extractors** (RAW → structured):

| Step | Input → Output | Описание |
|---|---|---|
| PdfTextExtractor | PDF → Markdown | Текст + структура + page mapping |
| PdfOcrExtractor | PDF → Markdown | С OCR для сканов |
| EpubExtractor | EPUB → Markdown | HTML chapters → markdown |
| ZimArticleExtractor | ZIM → Markdown | Wikipedia HTML → clean markdown |
| WikitextToMarkdown | XML → Markdown | MediaWiki разметка → markdown |
| HtmlToMarkdown | HTML → Markdown | Web страницы |
| ExifExtractor | Image → JSON | EXIF метаданные |
| VideoMetadataExtractor | Video → JSON | Duration, codec, GPS |
| GpxExtractor | GPX → GeoPoints | С Douglas-Peucker упрощением |
| SpeechToText | Audio → Text | Whisper STT |
| WikidataFilter | JSON → Entities | Filtered по языкам/предикатам |
| RdfParser | N-Triples/Turtle → Entities | via oxttl |
| TelegramParser | JSON → ChatMessages | Telegram export format |
| EmailParser | MBOX → Emails | С threading |
| BookmarkParser | JSON → Bookmarks | Chrome/Firefox |

**Enrichers** (добавляют знания через LLM/human):

| Step | Что делает | Fallback |
|---|---|---|
| ImageDescriber | Фото → текстовое описание (vision LLM) | skip |
| TextSummarizer | Длинный текст → резюме | skip |
| DataDescriber | Таблица → текстовое описание | skip |
| VideoDescriber | Keyframes → описание (vision LLM) | skip |
| AudioDescriber | Аудио → описание через LLM | skip |
| EntityExtractor | Текст → NER → entity mentions | skip |
| HumanAnnotation | Заметки от пользователя (batch/interactive) | skip |

**Transformers** (преобразования):

| Step | Что делает |
|---|---|
| ChatSegmenter | Messages → Conversations + Threads |
| EmailThreader | Messages → Threads via In-Reply-To |
| TrackSegmenter | GPS points → Day segments |
| TextCleaner | Boilerplate removal, whitespace normalization |
| LanguageDetector | Detect language |

**Writers** (output):

| Step | Что делает |
|---|---|
| ContentStoreWriter | → cluster-bundle zstd |
| GraphWriter | → Entity/Relation tables |
| CatalogWriter | → catalog.sqlite |

### 7.4 LLM Backend

```rust
enum LlmBackend {
    Ollama { model: String, host: String },
    Onnx { model_path: PathBuf },
    Candle { model_id: String, device: Device },
    OpenAiCompatible { base_url: String, api_key: Option<String>, model: String },
    Skip,  // пропустить шаг если LLM не нужен/недоступен
}
```

### 7.5 Pipeline Configuration (TOML)

```toml
# ═══ Google Photos: EXIF + optional LLM description ═══
[datasource.google-photos.pipeline]

[[datasource.google-photos.pipeline.steps]]
step = "exif_extractor"

[[datasource.google-photos.pipeline.steps]]
step = "image_describer"
enabled = true
llm = { backend = "ollama", model = "llava:13b" }
prompt = "Describe this photo in 2-3 sentences."
fallback = "skip"    # без LLM — хранится только EXIF

[[datasource.google-photos.pipeline.steps]]
step = "content_store_writer"


# ═══ Wikipedia: простой, без LLM ═══
[datasource.wikipedia-en.pipeline]

[[datasource.wikipedia-en.pipeline.steps]]
step = "zim_article_extractor"
to_markdown = true
extract_images = false

[[datasource.wikipedia-en.pipeline.steps]]
step = "content_store_writer"


# ═══ Telegram: с STT для голосовых ═══
[datasource.telegram.pipeline]

[[datasource.telegram.pipeline.steps]]
step = "telegram_parser"

[[datasource.telegram.pipeline.steps]]
step = "speech_to_text"
model = "whisper-small"
filter = "voice_messages_only"
fallback = "skip"

[[datasource.telegram.pipeline.steps]]
step = "chat_segmenter"
silence_threshold = "2h"
window_size = 10

[[datasource.telegram.pipeline.steps]]
step = "content_store_writer"


# ═══ Видео: metadata + keyframes + transcript ═══
[datasource.home-videos.pipeline]

[[datasource.home-videos.pipeline.steps]]
step = "video_metadata_extractor"

[[datasource.home-videos.pipeline.steps]]
step = "video_describer"
llm = { backend = "ollama", model = "llava:13b" }
keyframe_interval_s = 30
fallback = "skip"

[[datasource.home-videos.pipeline.steps]]
step = "speech_to_text"
model = "whisper-medium"
fallback = "skip"

[[datasource.home-videos.pipeline.steps]]
step = "content_store_writer"


# ═══ Закладки с человеческими аннотациями ═══
[datasource.bookmarks.pipeline]

[[datasource.bookmarks.pipeline.steps]]
step = "bookmark_parser"

[[datasource.bookmarks.pipeline.steps]]
step = "human_annotation"
annotations_file = "~/notes/bookmark-annotations.json"

[[datasource.bookmarks.pipeline.steps]]
step = "content_store_writer"
```

### 7.6 Pipeline Execution Model

```rust
struct PipelineStepConfig {
    step: Box<dyn PipelineStep>,
    filter: Option<DocumentFilter>,  // только определённые документы
    fallback: StepFallback,          // Skip | SkipDocument | Abort | Retry
    enabled: bool,
}

enum StepFallback {
    Skip,                            // пропустить шаг, продолжить
    SkipDocument,                    // пропустить документ
    Abort,                           // остановить pipeline
    Retry { max_attempts: u32, delay: Duration },
}
```

LLM-шаги можно запускать **позже** для уже оптимизированных данных:
```bash
lokb enrich google-photos --step image_describer --llm ollama:llava
```

---

## 8. Takeout Import

### 8.1 Поддерживаемые платформы

```
Google Takeout: Gmail, Google Chat, Photos (metadata), Location History,
  Maps, Calendar, Contacts, Chrome (bookmarks, history), YouTube, Keep, Fit

Apple: Photos, Notes, Health, Contacts, Calendar

Meta: Facebook Messages, Instagram, Photos

Messaging: Telegram, WhatsApp, Slack, Discord, Signal

Notes: Obsidian, Notion, Apple Notes

Other: Twitter/X archive
```

### 8.2 Takeout Dispatcher

Один Takeout ZIP → несколько DataSources автоматически:

```
Google Takeout
  → personal/google-email      (PersonalConfig)
  → personal/google-photos     (MediaMetaConfig)
  → personal/google-location   (MediaMetaConfig)
  → personal/google-bookmarks  (PersonalConfig)
  → personal/google-keep       (PersonalConfig)
```

```bash
lokb takeout import ~/takeout.zip --platform google \
  --include gmail,photos,location,bookmarks,keep \
  --exclude youtube,fit
```

---

## 9. Search & Access

### 9.1 Методы доступа

| Метод | Индекс | Latency | Требует vectors | Доступен |
|---|---|---|---|---|
| **Browse/Read** | Content Store | <500ms | Нет | Сразу |
| **Keyword (FTS)** | Tantivy BM25 | <50ms | Нет | Сразу |
| **Entity lookup** | Entity table | <10ms | Нет | После graph |
| **Graph navigation** | Relation traversal | <50ms | Нет | После graph |
| **Geo search** | Spatial index | <100ms | Нет | Сразу |
| **Timeline** | BTree on timestamp | <50ms | Нет | Сразу |
| **Semantic** | LanceDB IVF-PQ | <200ms | Да | После embedding |
| **Hybrid** | FTS + Vector RRF | <300ms | Да | После embedding |

### 9.2 Порядок доступности после ingestion

| Время | Что доступно |
|---|---|
| ~0s | Чтение документов (Content Store) |
| ~1-5 мин | Keyword search (FTS), Geo, Timeline |
| ~5-30 мин | Entity cards, Graph navigation (из RDF) |
| ~1-20 часов | Semantic search, Hybrid search (embeddings) |

### 9.3 Hybrid Ranking

RRF: `score(doc) = Σ 1 / (60 + rank_i(doc))`
Параметр `hybrid_alpha` (0.0-1.0): баланс vector vs FTS. Default 0.7.

### 9.4 Search Modes

```rust
enum SearchMode {
    Quick,   // FTS only, top-5, <50ms
    Normal,  // Hybrid, top-20, <300ms
    Deep,    // Hybrid + graph expansion + entity cards + reranking
    Auto,    // Система выбирает
}
```

### 9.5 Fact Lookup (для LLM)

```
Запрос: "population of Paris"

1. Entity lookup → Entity:Paris → Relations → population = 2.1M
   ✓ Structured answer + citation [Wikidata Q90, P1082]
2. Fallback: hybrid search → top chunk + citation
3. Fallback: FTS → keyword match + citation
```

Structured facts из knowledge graph preferred. Text search — fallback.

### 9.6 Source Viewer

Рендер полного документа с подсветкой найденного фрагмента.
Terminal (ratatui) или Browser (axum HTTP server).

Для чатов: thread view с контекстом.
Для медиа: metadata card + ссылка на оригинальный файл.

---

## 10. CLI + Skills

### 10.1 Source Management

```bash
# ═══ Добавление источников ═══
lokb source add wikipedia-en --raw ~/wiki.zim --format zim \
  --class public --raw-retention delete
lokb source add books --raw ~/library/ --format pdf-dir \
  --class personal --raw-retention keep
lokb source add photos --raw ~/Photos/ --format exif-dir \
  --class personal --raw-retention external
lokb source add wikidata --raw ~/wikidata.json.gz --format wikidata-json \
  --class public --raw-retention delete --languages en,ru
lokb source add telegram --raw ~/tg_export/ --format telegram-export \
  --class personal
lokb source add notes --raw ~/obsidian/ --format markdown-dir \
  --class personal --watch

# ═══ Takeout ═══
lokb takeout import ~/takeout.zip --platform google
lokb takeout import ~/apple-data/ --platform apple

# ═══ Управление ═══
lokb source list
lokb source status wikipedia-en
lokb source update wikipedia-en --raw ~/wiki-2024-06.zim
lokb source optimize wikipedia-en    # перезапустить pipeline
lokb raw list
lokb raw delete wikidata
```

### 10.2 Search

```bash
# Прямой поиск
lokb search "quantum error correction"
lokb search "quantum error correction" --mode deep --source wikipedia-en

# По категориям
lokb search "Paris" --public-only
lokb search "restaurant" --personal-only
```

### 10.3 Skills — шаблоны поиска

```bash
lokb lookup "population of Paris"
lokb define "Schrödinger equation"
lokb fact-check "Einstein was born in Munich"
lokb research "CRISPR gene editing applications"
lokb personal "обсуждали ресторан" --after 2024-01-01
lokb nearby 48.858,2.294 --radius 1km
```

```toml
# ~/.config/lokb/skills/

[skill.lookup]
description = "Quick fact lookup"
sources = ["wikidata", "wikipedia-*"]
search_mode = "quick"
output = { format = "short", max_results = 3 }

[skill.fact-check]
description = "Find evidence for or against a claim"
sources = ["wikipedia-*", "wikidata", "arxiv"]
search_mode = "deep"
output = { format = "evidence", max_results = 10, include_citations = true }

[skill.research]
description = "Deep research on a topic"
sources = ["*"]
search_mode = "deep"
output = { format = "report", max_results = 50, group_by = "source" }

[skill.personal]
description = "Search personal data"
sources = ["telegram", "email", "notes"]
search_mode = "normal"
output = { format = "timeline", include_context = true }

[skill.nearby]
description = "What's around a location?"
sources = ["wikidata", "photos-meta", "gps-tracks"]
search_mode = "geo"
output = { format = "map", radius_km = 1.0 }

[skill.define]
description = "Define a term"
sources = ["wikipedia-*", "wikidata"]
search_mode = "quick"
output = { format = "definition" }
```

Пользователь может создавать свои Skills.

### 10.4 Read & Entity

```bash
lokb read wikipedia-en:Quantum_computing
lokb read wikipedia-en:Quantum_computing --section "Error correction"
lokb entity Paris
lokb entity Paris --relations --depth 2
lokb entity Paris --documents
```

### 10.5 Pipeline & Enrichment

```bash
lokb pipeline show google-photos
lokb pipeline run google-photos
lokb pipeline status
lokb pipeline rerun google-photos --step image_describer
lokb enrich google-photos --step image_describer --llm ollama:llava
lokb annotate <doc_id> "This is where we had our anniversary dinner"
```

### 10.6 Background & Storage

```bash
lokb embed start
lokb embed status
lokb embed pause
lokb embed priority wikipedia-en

lokb storage status
lokb storage compact
lokb cache clear
lokb export knowledge.tar.zst
lokb export knowledge.tar.zst --include-personal
```

### 10.7 Serve (опционально)

```bash
lokb serve                   # HTTP localhost:7890
lokb serve --mcp             # MCP сервер для LLM (Phase 5)
```

### 10.8 Pipe-friendly

```bash
lokb search "quantum" --format json | jq '.results[0].citation'
lokb read wikipedia-en:Quantum_computing | less
lokb lookup "capital of France" --format text | ollama run phi3 "Answer: $(cat)"
lokb research "CRISPR" --format markdown > crispr_research.md
```

---

## 11. Budget Manager

```toml
[storage]
raw_limit = "50GB"
raw_path = "/mnt/external/lokb-raw"  # опционально
source_limit = "30GB"
derived_limit = "60GB"
cache_limit = "15GB"

[datasource.wikipedia-en]
raw_retention = "delete_after_optimize"
raw_reacquire = { type = "download", url = "https://dumps.wikimedia.org/..." }
source_limit = "5GB"
derived_limit = "20GB"
priority = 200

[datasource.telegram]
raw_retention = "delete_after_optimize"
source_limit = "100MB"
derived_limit = "500MB"
priority = 250          # личные данные — максимальный приоритет
```

**При превышении derived_limit:**
1. Деградация: full float32 vectors → PQ (30x экономия)
2. Truncated dimensions: 384→256→128 (Matryoshka)
3. Удалить FTS index (оставить vector search)
4. Eviction: удалить derived низкоприоритетного DataSource

**Embedding модели:**

| Модель | Dim | Размер | CPU скорость | Языки |
|---|---|---|---|---|
| all-MiniLM-L6-v2 | 384 | 80MB | ~1000 ch/s | EN |
| multilingual-e5-small | 384 | 120MB | ~700 ch/s | 100+ |
| BGE-M3 | 1024 | 2.2GB | ~100 ch/s | 100+ |
| nomic-embed-text-v1.5 | 768 | 550MB | ~300 ch/s | EN |

Default: `multilingual-e5-small`.

---

## 12. Physical Storage Layout

```
~/.local/share/lokb/
├── config.toml
│
├── raw/                              # RAW SOURCE
│   ├── manifest.toml
│   ├── wikipedia-en/
│   │   └── wikipedia.zim             # удаляется после optimize
│   ├── books/
│   │   ├── book1.pdf                 # хранится
│   │   └── book2.epub
│   └── ...
│
├── source/                           # OPTIMIZED SOURCE
│   ├── manifest.toml
│   ├── wikipedia-en/
│   │   ├── source.toml               # DataSource config + pipeline
│   │   ├── optimize_log.toml
│   │   └── bundles/
│   │       ├── manifest.bin
│   │       ├── 0001.zst              # ~1000 articles per bundle
│   │       └── ...
│   ├── wikidata/
│   │   ├── entities.zst
│   │   └── relations.zst
│   └── ...
│
├── derived/                          # DERIVED
│   ├── state.toml
│   ├── lance/                        # LanceDB
│   │   ├── chunks/
│   │   ├── entities/
│   │   └── relations/
│   ├── fts/                          # Tantivy
│   └── catalog.sqlite
│
├── cache/                            # CACHE (LRU)
│   ├── rendered/
│   ├── decompressed/
│   └── queries/
│
└── models/
    └── multilingual-e5-small/
        └── model.onnx                # 120MB
```

---

## 13. Crate Structure

```
lokb/
├── Cargo.toml                        # workspace
├── crates/
│   ├── lokb-core/                    # типы, трейты, конфиг, budget
│   ├── lokb-storage/                 # Content Store + LanceDB + SQLite
│   ├── lokb-pipeline/                # pipeline framework (steps, executor)
│   ├── lokb-optimize/                # RAW → OPTIMIZED orchestrator
│   ├── lokb-ingest/                  # OPTIMIZED → DERIVED orchestrator
│   ├── lokb-parsers/                 # format-specific optimizers/steps
│   │   ├── src/zim.rs
│   │   ├── src/wikipedia_xml.rs
│   │   ├── src/rdf.rs               # via oxttl
│   │   ├── src/wikidata.rs
│   │   ├── src/pdf.rs
│   │   ├── src/epub.rs
│   │   ├── src/chat/                 # Telegram, Slack, WhatsApp, ...
│   │   ├── src/media.rs             # EXIF, GPX, video
│   │   ├── src/documents.rs         # Markdown, HTML, PlainText
│   │   └── src/takeout/             # Google, Apple, Meta dispatchers
│   ├── lokb-search/                  # query engine, hybrid, RRF, skills
│   ├── lokb-embed/                   # embedding models (ONNX/Candle)
│   ├── lokb-llm/                     # LLM backends (Ollama, ONNX, Candle)
│   ├── lokb-graph/                   # entity resolution, relations
│   ├── lokb-render/                  # Source Viewer (terminal + HTML)
│   ├── lokb-serve/                   # HTTP server (axum), future MCP
│   └── lokb-cli/                     # CLI (clap) + TUI (ratatui)
```

---

## 14. Key Dependencies

```toml
# Storage
lancedb = "0.23"
rusqlite = { version = "0.32", features = ["bundled"] }

# RDF (Oxigraph crates)
oxrdfio = "0.2"
oxttl = "0.2"
oxrdf = "0.3"

# ML
ort = { version = "2", features = ["load-dynamic"] }  # ONNX Runtime

# Text
pulldown-cmark = "0.12"
scraper = "0.21"
quick-xml = "0.37"
ammonia = "4"

# Compression & hashing
zstd = "0.13"
blake3 = "1"

# Async + HTTP
tokio = { version = "1", features = ["full"] }
axum = "0.8"

# CLI + TUI
clap = { version = "4", features = ["derive"] }
ratatui = "0.29"

# File watching
notify = "7"

# Data
arrow-array = "54"
arrow-schema = "54"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v7"] }
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
indicatif = "0.17"
```

---

## 15. Roadmap

### Phase 0 — Skeleton (1 неделя)
- [ ] Cargo workspace, 13 crates
- [ ] Core types: DataSource, Document, Chunk, Entity, Relation
- [ ] DataSourceClass: Public / Personal
- [ ] Config parsing (TOML), Budget Manager
- [ ] Filesystem layout init (raw/source/derived/cache)
- [ ] Trait definitions: PipelineStep, SourceOptimizer, Chunker, Searcher

### Phase 1 — Pipeline + Read + FTS (5 недель)
- [ ] **Pipeline framework** (step trait, executor, TOML config)
- [ ] **Steps:** MarkdownPassthrough, PlainTextExtractor, HtmlToMarkdown
- [ ] **Steps:** TextCleaner, LanguageDetector
- [ ] **Steps:** ContentStoreWriter, CatalogWriter
- [ ] **Content Store:** cluster-bundle zstd writer/reader
- [ ] **Catalog:** SQLite (documents, datasources)
- [ ] **Chunker:** semantic splitting (headers, paragraphs)
- [ ] **FTS:** Tantivy index, BM25 keyword search
- [ ] **Source Viewer:** terminal renderer with highlight
- [ ] **CLI:** `source add/list/status`, `search`, `read`, `storage status`
- [ ] **Skills:** `lookup`, `define` (FTS-only)
- [ ] RAW management: retention policies, delete after optimize
- [ ] Budget tracking per layer per datasource
- [ ] HumanAnnotation step (batch from file)
- [ ] ✅ Человек может: добавить markdown/txt, искать, читать

### Phase 2 — Wikipedia + Embeddings (5 недель)
- [ ] **ZimOptimizer** (Wikipedia ZIM → clean markdown)
- [ ] **WikiXmlOptimizer** (fallback)
- [ ] **PdfOptimizer, EpubOptimizer**
- [ ] Background Embedder (ONNX, multilingual-e5-small)
- [ ] Semantic + Hybrid search (RRF, hybridalpha)
- [ ] PQ storage, budget-aware vector degradation
- [ ] HTTP Source Viewer (axum + browser rendering)
- [ ] Progressive loading (popular articles first)
- [ ] SearchMode: Quick / Normal / Deep
- [ ] Skills: `research`, `fact-check`
- [ ] `lokb export/import` (portable KB, public only by default)

### Phase 3 — Knowledge Graph (3 недели)
- [ ] **WikidataOptimizer** (JSON → filtered Entity/Relation)
- [ ] **RdfOptimizer** (oxttl → Entity/Relation)
- [ ] Entity/Relation tables (LanceDB + SQLite)
- [ ] Entity cards, entity resolution between DataSources
- [ ] Graph navigation: `lokb entity`, relations, path finding
- [ ] Graph expansion в Deep search mode
- [ ] Fact Lookup (structured → text fallback)
- [ ] Semantic graph edges (txtai-style: embedding proximity)

### Phase 4 — Personal Data + Takeout (4 недели)
- [ ] **TakeoutDispatcher** (Google, Apple, Meta)
- [ ] **TelegramOptimizer** (segmentation + normalization)
- [ ] **EmailOptimizer** (MBOX → threads)
- [ ] **PhotoOptimizer** (EXIF extraction)
- [ ] **GpsOptimizer** (GPX → simplified segments)
- [ ] **GooglePhotosOptimizer** (metadata JSON)
- [ ] **GoogleLocationOptimizer** (Timeline JSON → GeoPoints)
- [ ] File watch (notify crate) для Obsidian vault
- [ ] Privacy levels + фильтрация
- [ ] Geo search, Timeline search
- [ ] Spatio-temporal entity linking (photos ↔ GPS ↔ chats)
- [ ] Skills: `personal`, `nearby`

### Phase 5 — LLM Integration (3 недели)
- [ ] **LLM backend abstraction** (Ollama, ONNX, Candle, OpenAI-compatible)
- [ ] **ImageDescriber** step (vision LLM)
- [ ] **SpeechToText** step (Whisper)
- [ ] **VideoDescriber** step
- [ ] **TextSummarizer** step
- [ ] `lokb enrich` command (run LLM steps on existing data)
- [ ] MCP Server (Model Context Protocol)
- [ ] Citation generation в LLM ответах
- [ ] `verify_claim` skill

### Phase 6 — Polish (2 недели)
- [ ] Incremental sync (delta updates)
- [ ] arXiv, Stack Overflow optimizers
- [ ] Compaction, storage optimization
- [ ] Benchmarks, documentation
- [ ] Interactive HumanAnnotation mode

---

## 16. Нефункциональные требования

**Latency:**
| Операция | Target |
|---|---|
| Document read (cache hit) | <100ms |
| Document read (cache miss) | <500ms |
| FTS search (50M chunks) | <50ms |
| Vector search (50M chunks) | <200ms |
| Hybrid search | <300ms |
| Entity lookup | <10ms |

**Throughput:**
| Операция | Target |
|---|---|
| Optimize (PDF) | ~1K pages/s |
| Optimize (Wiki articles) | ~10K articles/s |
| Chunk + FTS index | >10K docs/s |
| Embedding (CPU) | >500 chunks/s |
| Embedding (GPU) | >5K chunks/s |

**Storage (typical config):**
```
                RAW(deletable) OPTIMIZED DERIVED  ON DISK
Wikipedia EN    25GB→del       4GB       20GB     24GB
Wikidata        90GB→del       2.5GB     3GB      5.5GB
Books (1000)    10GB keep      2GB       8GB      20GB
arXiv           3GB→del        0.5GB     3GB      3.5GB
Telegram        500MB→del      50MB      300MB    350MB
Photos 50K      external       250MB     500MB    750MB
Notes 1K        external       100MB     500MB    600MB
GPS tracks      external       50MB      100MB    150MB
Model                                            120MB
──────────────────────────────────────────────────────
TOTAL (no RAW):                ~10GB     ~36GB    ~55GB
With PQ vectors:               ~10GB     ~16GB    ~35GB
```

**System:**
- Startup: <2s до готовности к поиску
- Memory: <500MB RSS (mmap)
- Binary: <30MB без модели, <150MB с bundled ONNX
- Platforms: macOS (arm64, x86_64), Linux (x86_64, arm64), Windows

**Portability:** Скопировать `~/.local/share/lokb/` = полная миграция.

---

## 17. Incorporated Ideas from Open Source

| Проект | Что взято | Фаза |
|---|---|---|
| **Kiwix/ZIM** | Cluster-bundle compression; ZIM как RAW SOURCE | P1, P2 |
| **txtai** | Semantic graph from embeddings; hybridalpha; SQL over embeddings; portable KB export | P2, P3 |
| **Khoj** | File watch; citation format | P4, P5 |
| **Perplexica** | Source cards UI; Quick/Normal/Deep modes | P1, P2 |
| **Oxigraph** | `oxttl`/`oxrdfio` crates для RDF parsing | P3 |
| **Lee Butterman** | PQ 384→48 bytes (30x); progressive loading; brute-force <10M | P2 |
