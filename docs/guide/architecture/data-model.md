# Data Model

## Document — иерархическая единица

Основная единица хранения. Документы образуют деревья:

```
Book(Root)         → Chapter(Section)     → Chunks
Conversation(Root) → Thread(Section)      → Chunks (message windows)
EmailThread(Root)  → Email(Section)       → Chunks
Article(Root)      → Chunks (flat)
```

Ключевые поля: `id` (UUID v7), `source_id`, `parent_id`, `content_type`, `content_hash` (Blake3), `language`, `entity_links`.

## Chunk — единица поиска

Фрагмент документа, индексируемый для поиска. **Vector — optional**: FTS и browse доступны сразу, embeddings вычисляются в фоне.

Ключевые поля: `text`, `vector` (Optional), `byte_start`/`byte_end`, `section_path`, `content_type`, `language`, `timestamp`, `latitude`/`longitude`, `privacy_level`, `entity_ids`.

## Entity — узел Knowledge Graph

Сущность из Wikidata, NER или personal data:

```
Entity:Paris → {
  canonical_name: "Paris",
  labels: { en: ["Paris"], fr: ["Paris"], ru: ["Париж"] },
  external_ids: { wikidata: "Q90", wikipedia-en: "Paris" },
  types: [City, Capital],
  latitude: 48.856, longitude: 2.352
}
```

## Relation — ребро графа

Связь между entities: `subject → predicate → object` с confidence и qualifiers.

Типы предикатов: Ontological (из RDF), Relational, Temporal, Spatial, Mentions, CoOccurrence, SentBy, VisitedAt, SemanticEdge (embedding proximity).

## DataSource — источник данных

```
Public  { license, web_url_template, exportable: true }
Personal { owner, platform, contains_pii, exportable: false }
```

**Privacy Levels:** Public (0) → Internal (1) → Private (2) → Secret (3).

Правило: Personal → Public линкуется асимметрично ("мой чат mentions Paris" → link to Entity:Paris, но не наоборот).
