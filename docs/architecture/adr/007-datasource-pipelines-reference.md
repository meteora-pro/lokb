# ADR-007: DataSource pipelines reference — concrete implementations

**Status:** Accepted
**Date:** 2026-03-14

## Context

ADR-001..006 описывают архитектуру абстрактно. Нужно валидировать её на реальных примерах: конкретные data sources, их optimize/enrichment pipelines, update/dedup стратегии, cross-source linking. Это также служит reference для планирования реализации.

## Decision

Описание 11 конкретных data sources, полного pipeline для каждого, и выявленные паттерны.

### Source catalog

| # | Source | RAW size | Compression | external_id | Sync | Phase |
|---|---|---|---|---|---|---|
| 1 | Wikipedia ZIM | 25 GB | 6x | article title | FullReload/3mo | P2 |
| 2 | Wikidata RDF | 90 GB | 36x | QID (Q90) | FullReload/1mo | P3 |
| 3 | GPX tracks | ~100 MB | 4000x | filename | FileWatch | P4 |
| 4 | Email MBOX | 2 GB | 10x | Message-ID header | Incremental | P4 |
| 5 | Telegram export | 500 MB | 10x | chat+msg_id | Incremental | P4 |
| 6 | Browser history | 50 MB | 10x | URL (normalized) | Incremental | P4 |
| 7 | Photo library | 400 GB ext | 16000x | filename+datetime | FileWatch | P4 |
| 8 | arXiv papers | 4 GB | 5x | arXiv ID / DOI | Incremental | P6 |
| 9 | Library changelogs | ~100 MB | 3x | lib@version | Incremental | P6 |
| 10 | CVE database | 200 MB/yr | 7x | CVE ID | Incremental/daily | P6 |
| 11 | Statistical data | varies | 10-1000x | provider:dataset:ver | Incremental/yearly | P6 |

---

### 1. Wikipedia ZIM

**Optimize Pipeline:**
```
ZimReader → ArticleExtractor → HtmlToMarkdown → TextCleaner → ContentStoreWriter
```

- ZimReader: iterate ZIM entries, yield article HTML
- ArticleExtractor: skip redirects, disambiguation, meta pages
- HtmlToMarkdown: MediaWiki HTML → clean markdown, preserve section structure
- TextCleaner: remove boilerplate (nav, footnote refs), normalize whitespace
- Loss: images (only alt-text), CSS/JS, navigation. Keep: text, sections, `[[wikilinks]]`

**Enrichment:** SemanticChunker → FtsIndexer → EmbeddingIndexer → WikilinkEntityExtractor

WikilinkEntityExtractor is format-specific: `[[Paris]]` → Entity:Paris. No NER needed — wikilinks preserved in OPTIMIZED markdown.

**Update:** FullReload every 3 months. Diff by article title + Blake3 hash. Typical: 95% unchanged → skip.

**Hierarchy:** Article (depth=0) → flat chunks.

---

### 2. Wikidata RDF

**Optimize Pipeline:**
```
WikidataJsonReader → LanguageFilter → PropertyFilter → EntityNormalizer → ContentStoreWriter + GraphWriter
```

- LanguageFilter: keep labels/descriptions for en, ru + 5 languages (from ~400)
- PropertyFilter: keep ~200 useful properties (population, coordinates, instance_of) from ~10000
- Output: Entity + Relation structs (already structured, minimal text)
- Loss: sitelinks, non-selected languages/properties, most qualifiers

**Enrichment:** EntityFtsIndexer → EntityEmbedder → GeoIndexer. Minimal — already structured.

**Update:** FullReload monthly. Future: Wikidata recent changes API for incremental.

**Key role:** Hub for entity resolution. QIDs are lingua franca across all sources.

---

### 3. Personal GPX tracks

**Optimize Pipeline:**
```
GpxParser → TrackSimplifier → SegmentSplitter → GeoTextGenerator → ContentStoreWriter
```

- TrackSimplifier: Douglas-Peucker, 5000→500 points (preserve shape)
- SegmentSplitter: split by day / 2h pauses
- GeoTextGenerator: points → "Track from 48.856°N to 48.804°N. Duration: 3h 20m. Distance: 25.3 km"
- Loss: individual points (simplified), heart rate, cadence. Keep: route shape, timestamps, stats

**Enrichment:** FtsIndexer → GeoIndexer → ReverseGeocoder

ReverseGeocoder: coordinates → place names via Wikidata spatial index (offline). "48.856°N, 2.352°E" → "near Eiffel Tower, Paris".

**Update:** FileWatch(5s). New GPX → auto-import.

**Hierarchy:** Track file (depth=0) → day segments (depth=1) → chunks.

---

### 4. Personal emails (MBOX)

**Optimize Pipeline:**
```
MboxParser → EmailThreader → HeaderExtractor → HtmlBodyToText → ContentStoreWriter
```

- EmailThreader: group by In-Reply-To / References headers → threads
- HeaderExtractor: from, to, cc, date, subject, message-id
- HtmlBodyToText: strip signatures, quoted replies, HTML formatting
- Loss: HTML formatting, images/attachments (keep filenames). Keep: text, thread structure, headers

**Enrichment:** WindowChunker (5 emails/step 3) → FtsIndexer → EmbeddingIndexer → ContactEntityExtractor

ContactEntityExtractor: From/To/CC → Person entities. Structured data, not NER: `"Alice Smith <alice@example.com>"` → Entity.

**Update:** Incremental by Message-ID. Re-export → skip existing.

**Hierarchy:** EmailThread (depth=0) → individual emails (depth=1) → chunks.

---

### 5. Telegram export

**Optimize Pipeline:**
```
TelegramJsonParser → ChatSegmenter → MessageWindowBuilder → ContentStoreWriter
```

- ChatSegmenter: segment by reply-chains, 2h silence gaps, topic changes
- MessageWindowBuilder: sliding window 10 msg / step 5 → chunks (chunking happens in optimize, not enrichment)
- Output format: `"[Chat Name, 2024-03-15]\nAlice [14:20]: text\nBob [14:21]: reply"`
- Loss: media (only references). Keep: text, timestamps, sender, reply structure

**Enrichment:** FtsIndexer → EmbeddingIndexer → ContactEntityExtractor → SpeechToText (optional, LLM)

SpeechToText: voice messages → text via Whisper. Optional, fallback=skip.

**Update:** Incremental by message_id. Track last_seen_id per chat. Edited messages: same id + different content → update.

**Hierarchy:** Chat (depth=0, Conversation) → Segments (depth=1, Thread) → message windows (chunks).

---

### 6. Browser search history

**Optimize Pipeline:**
```
BrowserHistoryParser → UrlDeduplicator → PageTitleExtractor → ContentStoreWriter
```

- BrowserHistoryParser: read Chrome SQLite / Firefox places.sqlite
- UrlDeduplicator: group by URL (normalized), keep first+last visit, count
- Output: "URL: ...\nTitle: ...\nFirst visit: ...\nLast visit: ...\nCount: 5"

**Enrichment:** FtsIndexer → UrlEntityExtractor → TimelineIndexer

UrlEntityExtractor: `wikipedia.org/wiki/Paris` → Entity:Paris. Pattern matching, no LLM.

**Update:** Incremental. `last_visit_time > checkpoint` → new entries only.

**Hierarchy:** Flat (one record per URL).

---

### 7. Photo library

**Optimize Pipeline:**
```
ExifExtractor → GeoResolver → PhotoTextGenerator → ContentStoreWriter
```

Optional enrichment in optimize:
```
→ ImageDescriber (vision LLM): "Sunset at Eiffel Tower from Trocadéro, people walking"
```

- ExifExtractor: datetime, GPS, camera, lens, exposure
- GeoResolver: GPS → place name via Wikidata spatial index
- PhotoTextGenerator: structured text description
- RAW retention: ExternalReference (don't copy 400GB of photos, just link)
- Loss: pixel data (not stored). Keep: all metadata + text description

**Enrichment:** FtsIndexer → EmbeddingIndexer → GeoIndexer → TimelineIndexer → FaceRecognizer (future)

**Update:** FileWatch(30s). Hash of EXIF (not pixels) for change detection.

**Hierarchy:** Flat (one record per photo).

---

### 8. Scientific publications (arXiv)

**Optimize Pipeline:**
```
ArxivMetadataParser → AbstractExtractor → LatexCleaner → CitationGraphWriter → ContentStoreWriter
```

Optional for full text:
```
PdfExtractor → SectionSplitter → LatexMathNormalizer → ContentStoreWriter
```

- LatexCleaner: LaTeX formulas → readable text or preserve as `$E=mc^2$`
- CitationGraphWriter: references → Relations (paper A cites paper B)
- Loss: figures, tables (only captions). Keep: text, sections, math, references, authors

**Enrichment:** SemanticChunker → FtsIndexer → EmbeddingIndexer → AuthorEntityExtractor → CitationIndexer → CategoryTagger

CitationIndexer: unique — builds citation graph. Enables "papers citing this work", "seminal papers by citation count".

**Update:** Incremental by arXiv ID. ~500 new papers/day. Revised versions: same external_id, new hash → update.

**Hierarchy:** Paper (depth=0) → sections if full PDF (depth=1) → chunks.

---

### 9. Library changelogs + API changes

**Optimize Pipeline:**
```
ChangelogParser → VersionSplitter → BreakingChangeExtractor → ContentStoreWriter
```

Or:
```
GitHubReleasesParser → ReleaseNormalizer → ContentStoreWriter
```

- VersionSplitter: each version = separate Document
- BreakingChangeExtractor: detect "Breaking Changes", "Deprecated" sections → annotations
- ApiDiffParser: JSON diff → "Added: useEffect. Removed: componentWillMount"

**Enrichment:** FtsIndexer → EmbeddingIndexer → ApiSymbolExtractor → BreakingChangeAnnotator

ApiSymbolExtractor: extract API names from text → Entity (ApiSymbol). `"React.useEffect"` with version introduced/deprecated.

Relations: `react@18.0.0 --deprecates--> ReactDOM.render`, `react@18.0.0 --introduces--> createRoot`.

**Update:** Incremental by version. Versions are immutable — only new ones.

**Hierarchy:** Library (depth=0) → versions (depth=1) → chunks.

---

### 10. CVE vulnerability database

**Optimize Pipeline:**
```
NvdJsonParser → CveSeverityFilter → AffectedProductMapper → ContentStoreWriter + GraphWriter
```

- AffectedProductMapper: CPE strings → readable product names + version ranges
- Output includes: description, CVSS score, affected products, references, timeline

**Enrichment:** FtsIndexer → EmbeddingIndexer → ProductEntityExtractor → SeverityAnnotator → ExploitStatusAnnotator

ProductEntityExtractor: CPE → Entity (Product) with version ranges.
ExploitStatusAnnotator: cross-reference with CISA KEV catalog → "known exploited in the wild".

**Update:** Incremental/daily via NVD modified feed. CVEs get updated (new info, score changes). Rejected CVEs → soft delete.

**Hierarchy:** Flat (one CVE per document).

---

### 11. Public statistical data

**Optimize Pipeline:**
```
CsvParser → TableDescriber → StatisticalSummarizer → ContentStoreWriter + StructuredDataWriter
```

- TableDescriber: schema + sample → text description
- StatisticalSummarizer: compute stats → key facts as text
- StructuredDataWriter: raw data → SQLite copy in derived for exact queries
- Core idea: 50MB CSV → 5KB text description + queryable SQLite copy

**Enrichment:** FtsIndexer → EmbeddingIndexer → GeoEntityExtractor → FactExtractor → StructuredQueryIndex

FactExtractor: key numbers → Relations. `France --population--> 67800000 [year:2023, source:worldbank]`. Enables `lokb lookup "population of France"` → structured answer.

StructuredQueryIndex: SQLite copy for exact SQL queries against the data.

**Update:** Incremental/yearly. New rows appended → new summary → update hash.

**Hierarchy:** Dataset (depth=0) → flat chunks.

---

## Выявленные паттерны

### Pattern 1: Format-specific entity extraction without NER

Every source has structured ways to extract entities without LLM-based NER:

| Source | Extractor | Method |
|---|---|---|
| Wikipedia | WikilinkEntityExtractor | Parse `[[wikilinks]]` |
| Wikidata | (already entities) | Structured data |
| Email | ContactEntityExtractor | Parse From/To/CC headers |
| Telegram | ContactEntityExtractor | Parse from field |
| Browser | UrlEntityExtractor | Parse Wikipedia URLs |
| arXiv | AuthorEntityExtractor | Parse author metadata |
| Changelogs | ApiSymbolExtractor | Parse API names in text |
| CVE | ProductEntityExtractor | Parse CPE strings |
| Statistics | GeoEntityExtractor | Parse country/city names |
| Photos | (via GeoResolver) | Parse GPS coordinates |
| GPX | (via ReverseGeocoder) | Parse GPS coordinates |

NER (LLM-based) is fallback when structured extraction is not possible.

### Pattern 2: Three cost tiers of enrichment

```
CHEAP (minutes)          MEDIUM (tens of min)       EXPENSIVE (hours)
──────────────           ─────────────────────      ────────────────
FtsIndexer               EmbeddingIndexer            ImageDescriber
GeoIndexer               CitationIndexer             SpeechToText
TimelineIndexer          EntityResolver              FaceRecognizer
FactExtractor            SemanticGraphBuilder        Full PDF extraction
*EntityExtractors        StructuredQueryIndex        NER via LLM
```

### Pattern 3: Cross-source linking through three dimensions

```
                    Wikidata (hub entity)
                    /    |    \
           Wikipedia  Photos  GPX tracks
                |       |       |
              arXiv   Telegram  Email
                |       |
              CVE    Changelogs
                        |
                    Browser history
                        |
                    Statistics
```

Three dimensions for linking:
- **Entity** — Person, Place, Product, ApiSymbol, Paper → via EntityResolver
- **Space** — GPS coordinates → via SpatioTemporalLinker
- **Time** — timestamps → via TimelineCorrelator

### Pattern 4: Chunking determined by content type, not source format

After OPTIMIZED, chunking is format-agnostic:

| ContentType | Chunker | Strategy |
|---|---|---|
| Article, Paper, Note | SemanticChunker | By headers + paragraphs, ~512 tokens |
| Book | SemanticChunker | By chapters → sections → paragraphs |
| Conversation, Thread | WindowChunker | Already windowed in optimize step |
| Email, EmailThread | WindowChunker | 5 emails / step 3 |
| MediaMeta, GpsSegment | FixedChunker | One record = one chunk |
| Record (CVE, history) | FixedChunker | One record = one chunk |

### Pattern 5: Some sources require optimize-time chunking

Telegram and chat sources chunk during optimize (MessageWindowBuilder), not during enrichment. This is because chat segmentation requires understanding the conversation flow, which is format-specific knowledge.

For all other sources, chunking happens in enrichment (format-agnostic).

## Implementation roadmap

| Phase | Sources | OptimizeSteps | EnrichmentSteps |
|---|---|---|---|
| **P1** | Markdown dir (existing) | MarkdownPassthrough, TextCleaner | SemanticChunker, FtsIndexer |
| **P2** | Wikipedia ZIM, PDF, EPUB | ZimReader+ArticleExtractor, PdfExtractor, EpubExtractor | EmbeddingIndexer, WikilinkEntityExtractor |
| **P3** | Wikidata RDF | WikidataJsonReader+Filters, RdfParser | EntityFtsIndexer, GeoIndexer, EntityResolver |
| **P4** | Telegram, Email, Photos, GPX, Browser | TelegramParser, MboxParser, ExifExtractor, GpxParser, BrowserHistoryParser | ContactEntityExtractor, ReverseGeocoder, SpatioTemporalLinker |
| **P5** | (LLM enrichment) | — | ImageDescriber, SpeechToText, NER |
| **P6** | arXiv, Changelogs, CVE, Statistics | ArxivParser, ChangelogParser, NvdParser, CsvParser | CitationIndexer, ApiSymbolExtractor, FactExtractor |

## Consequences

**Validated by concrete examples:**
- Four-layer model works for all 11 sources
- Dual pipeline separation confirmed: optimize is format-specific, enrichment is format-agnostic (with pattern 5 exception for chats)
- Entity extraction without NER covers most cases
- Three linking dimensions (entity, space, time) connect all sources
- Incremental update model handles all sync strategies

**New insights:**
- StructuredQueryIndex (SQLite copy for exact queries) is a new enrichment type not originally planned
- Chat sources need optimize-time chunking (exception to the rule)
- ReverseGeocoder depends on Wikidata being loaded first (cross-source dependency in enrichment)

**Risks:**
- 7 OptimizeStep implementations in Phase 4 alone — need good test fixtures and shared utilities
- Cross-source dependencies (ReverseGeocoder needs Wikidata) complicate task scheduling
- Statistical data StructuredQueryIndex blurs the line between OPTIMIZED and DERIVED
