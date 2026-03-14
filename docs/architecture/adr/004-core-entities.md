# ADR-004: Core domain entities

**Status:** Accepted
**Date:** 2026-03-14

## Context

Нужно определить базовые сущности доменной модели, их связи и ответственности. Модель должна поддерживать: публичные и личные данные, иерархические документы, поиск по фрагментам, knowledge graph.

## Decision

Пять core entities:

### DataSource

Источник данных. Определяет откуда данные, какого класса, как синхронизировать.

```rust
struct DataSource {
    id: DataSourceId,
    name: String,
    class: DataSourceClass,
    format: String,
    sync_strategy: SyncStrategy,
    raw_retention: RawRetention,
    privacy_policy: PrivacyPolicy,
    priority: u32,               // для budget decisions (250 = personal, 100 = public)
}

enum DataSourceClass {
    Public { license: String, web_url_template: Option<String>, exportable: bool },
    Personal { owner: String, platform: String, contains_pii: bool, exportable: bool },
}
```

### Document

Иерархическая единица контента. Образует деревья:

```
Book(Root) → Chapter(Section) → Chunks
Conversation(Root) → Thread(Section) → Chunks
Article(Root) → Chunks (flat)
```

```rust
struct Document {
    id: DocumentId,              // UUID v7
    source_id: DataSourceId,
    external_id: String,         // оригинальный ID в источнике
    parent_id: Option<DocumentId>,
    depth: u8,                   // 0 = root
    title: String,
    content_type: ContentType,
    language: Option<Language>,
    content_hash: Blake3Hash,    // для invalidation tracking
    content_size: u64,
    created_at: DateTime<Utc>,
    indexed_at: DateTime<Utc>,
    source_ref: SourceRef,       // откуда пришёл (URL, file path, etc.)
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

### Chunk

Единица поиска. Фрагмент Document с optional embedding vector.

```rust
struct Chunk {
    id: ChunkId,
    document_id: DocumentId,
    source_id: DataSourceId,     // денормализовано для фильтрации
    chunk_index: u32,
    text: String,
    vector: Option<Vec<f32>>,    // OPTIONAL — вычисляется в фоне
    byte_start: u64,
    byte_end: u64,
    section_path: Option<String>,
    content_type: String,
    language: Option<String>,
    timestamp: Option<i64>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    privacy_level: u8,
    entity_ids: Vec<EntityId>,
}
```

**Ключевое:** `vector: Option` — FTS и browse доступны сразу. Semantic search подключается по мере вычисления embeddings в фоне.

### Entity

Узел knowledge graph. Сущность из Wikidata, NER или personal data.

```rust
struct Entity {
    id: EntityId,
    canonical_name: String,
    labels: HashMap<Language, Vec<String>>,  // multilingual aliases
    description: Option<String>,
    types: Vec<EntityType>,
    external_ids: HashMap<String, String>,   // wikidata: Q90, wikipedia-en: Paris
    name_vector: Option<Vec<f32>>,           // для fuzzy entity resolution
    latitude: Option<f64>,
    longitude: Option<f64>,
    mention_count: u32,
}
```

### Relation

Ребро knowledge graph.

```rust
struct Relation {
    subject_id: EntityId,
    predicate: PredicateId,
    object_id: EntityId,
    source_id: DataSourceId,     // откуда пришло
    confidence: f32,
    qualifiers: Option<HashMap<String, Value>>,
}
```

Типы предикатов: Ontological (из RDF), Relational, Temporal, Spatial, Mentions, CoOccurrence, SentBy, VisitedAt, SemanticEdge (embedding proximity).

### Связи между entities

```
DataSource ─1:N─► Document ─1:N─► Chunk
                      │               │
                      │ tree           │ entity_ids
                      │ (parent_id)    │
                      ▼               ▼
                  Document         Entity ◄──── Relation ────► Entity
```

### Privacy model

```rust
enum PrivacyLevel {
    Public = 0,     // Wikipedia → export OK
    Internal = 1,   // Книги → entity linking OK, no web URL
    Private = 2,    // Чаты → только local view
    Secret = 3,     // Не показывать без явного фильтра
}
```

Правило асимметричности: Personal → Public линкуется (чат mentions Paris → link to Entity:Paris), но Public не знает о Personal. При export: personal links исключаются.

### OptimizedDocument

Документ в OPTIMIZED слое — текст + аннотации:

```rust
struct OptimizedDocument {
    document: Document,
    text_content: String,
    text_format: TextFormat,         // Markdown | PlainText
    source_refs: Vec<SourceRef>,
    annotations: Vec<Annotation>,    // human + LLM + pipeline
}

struct Annotation {
    author: AnnotationAuthor,        // Human | Llm { model } | Pipeline { step }
    kind: AnnotationKind,            // Summary | Description | EntityMention | Tag
    text: String,
    anchor: Option<TextSpan>,        // к какому фрагменту привязана
    created_at: DateTime<Utc>,
}
```

Enrichment добавляет Annotations к OptimizedDocument (image description, NER mentions, summaries). Annotations мутируют OPTIMIZED, а не создают новый слой.

## Consequences

**Плюсы:**
- 5 сущностей покрывают все use cases (публичные, личные, граф, поиск)
- Chunk.vector = Optional — progressive enhancement
- Annotation model — enrichment расширяет документ, не создавая отдельный слой
- Privacy levels — чёткое разделение данных при export

**Минусы:**
- DocumentId как UUID v7 — больше места чем sequential ID
- Денормализация source_id в Chunk — дублирование, но нужно для фильтрации
- Entity resolution между sources — сложная задача (fuzzy matching)

## Alternatives considered

1. **Без Document (только Chunks):** теряем иерархию (Book→Chapter), нельзя "прочитать документ"
2. **Без Entity/Relation:** можно, но тогда нет knowledge graph, нет fact lookup
3. **Enum вместо trait для ContentType:** проще, но не расширяемо — выбрали enum, т.к. список типов конечен и известен
4. **Annotation как отдельная таблица:** вместо inline в OptimizedDocument — но тогда нужен join при каждом чтении
