# Pipeline

> See [ADR-002](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/002-dual-pipeline.md) and [ADR-005](https://github.com/meteora-pro/lokb/blob/main/docs/architecture/adr/005-extension-model.md) for full specification.

## Two pipelines

### Optimize Pipeline (RAW → OPTIMIZED)

Compresses with controlled loss. Format-specific.

```
RAW file → [Extract] → [Clean] → [Write to Content Store]
```

Trait: `OptimizeStep`

```rust
trait OptimizeStep: Send + Sync {
    fn name(&self) -> &str;
    fn estimate(&self, input_size: u64) -> OptimizeEstimate;
    async fn execute(&self, input: StepData, ctx: &PipelineContext) -> Result<StepData>;
}
```

### Enrichment Pipeline (OPTIMIZED → DERIVED)

Expands data. Format-agnostic (works on text).

Trait: `EnrichmentStep`

```rust
trait EnrichmentStep: Send + Sync {
    fn name(&self) -> &str;
    fn enrichment_kind(&self) -> EnrichmentKind;
    fn estimate(&self, chunk_count: u64) -> EnrichmentEstimate;
    async fn execute(&self, source: &dyn ChunkSource, ctx: &PipelineContext) -> Result<()>;
    fn supports_incremental(&self) -> bool;
    fn degrade_strategy(&self) -> Option<DegradeStrategy>;
}
```

### Cross-source Pipeline

Links data between sources. Trait: `CrossSourceStep`

## Built-in steps

### Optimize steps (format-specific)

| Step | Input → Output |
|---|---|
| ZimArticleExtractor | ZIM → Markdown |
| PdfTextExtractor | PDF → Markdown |
| EpubExtractor | EPUB → Markdown |
| TelegramParser | JSON → ChatMessages |
| RdfParser | N-Triples/Turtle → Entities |
| ExifExtractor | Image → JSON metadata |

### Enrichment steps (format-agnostic)

| Step | What it does | Cost |
|---|---|---|
| SemanticChunker | Text → chunks by headers/paragraphs | Cheap |
| FtsIndexer | Chunks → Tantivy BM25 index | Cheap |
| EmbeddingIndexer | Chunks → vectors (ONNX) | Expensive |
| EntityExtractor | Text → NER → entity mentions | Medium-Expensive |
| ImageDescriber | Photo → text description (vision LLM) | Expensive |

### Cross-source steps

| Step | What it does |
|---|---|
| EntityResolver | Match entities across sources |
| SpatioTemporalLinker | Link by coordinates + timestamps |
| SemanticGraphBuilder | Embedding proximity → graph edges |

## Configuration (TOML)

```toml
# Optimize — per source, format-specific
[datasource.wikipedia-en.optimize]
[[datasource.wikipedia-en.optimize.steps]]
step = "zim_article_extractor"
to_markdown = true

[[datasource.wikipedia-en.optimize.steps]]
step = "content_store_writer"

# Enrichment — global or per source
[enrichment]
preset = "full"  # fast (FTS only) | full | custom
```

## Fallback strategies

```rust
enum StepFallback {
    Skip,           // skip step, continue pipeline
    SkipDocument,   // skip this document
    Abort,          // stop pipeline
    Retry { max_attempts: u32, delay: Duration },
}
```

LLM steps always have `fallback = "skip"` — the pipeline works without LLM.
