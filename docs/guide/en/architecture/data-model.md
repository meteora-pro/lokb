# Data Model

> See [ADR-004](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/004-core-entities.md) for full specification.

## Document — hierarchical unit

The primary content unit. Documents form trees:

```
Book(Root)         → Chapter(Section)     → Chunks
Conversation(Root) → Thread(Section)      → Chunks (message windows)
EmailThread(Root)  → Email(Section)       → Chunks
Article(Root)      → Chunks (flat)
```

Key fields: `id` (UUID v7), `source_id`, `parent_id`, `content_type`, `content_hash` (Blake3), `language`, `entity_links`.

## Chunk — search unit

A document fragment indexed for search. **Vector is optional**: FTS and browse are available immediately; embeddings are computed in the background.

Key fields: `text`, `vector` (Optional), `byte_start`/`byte_end`, `section_path`, `content_type`, `language`, `timestamp`, `latitude`/`longitude`, `privacy_level`, `entity_ids`.

## Entity — knowledge graph node

An entity from Wikidata, NER, or personal data:

```
Entity:Paris → {
  canonical_name: "Paris",
  labels: { en: ["Paris"], fr: ["Paris"], ru: ["Париж"] },
  external_ids: { wikidata: "Q90", wikipedia-en: "Paris" },
  types: [City, Capital],
  latitude: 48.856, longitude: 2.352
}
```

## Relation — graph edge

A link between entities: `subject → predicate → object` with confidence and qualifiers.

Predicate types: Ontological (from RDF), Relational, Temporal, Spatial, Mentions, CoOccurrence, SentBy, VisitedAt, SemanticEdge (embedding proximity).

## DataSource

```
Public  { license, web_url_template, exportable: true }
Personal { owner, platform, contains_pii, exportable: false }
```

**Privacy Levels:** Public (0) → Internal (1) → Private (2) → Secret (3).

Asymmetric linking rule: Personal → Public links are allowed ("my chat mentions Paris" → link to Entity:Paris), but Public sources don't know about Personal data. Personal links are excluded on export.
