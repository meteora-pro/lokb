# ADR-005: Extension model — traits and plugin points

**Status:** Accepted
**Date:** 2026-03-14

## Context

lokb должен поддерживать десятки форматов данных (ZIM, PDF, EPUB, Telegram, email, GPS, ...), несколько поисковых бэкендов, LLM/embedding модели. Нужна расширяемая архитектура, где добавление нового формата или индекса не требует изменения существующего кода.

## Decision

Семь точек расширения через traits. Каждый trait — один тип расширения:

### 1. OptimizeStep — новые форматы данных

```rust
trait OptimizeStep: Send + Sync {
    fn name(&self) -> &str;
    fn input_type(&self) -> StepDataType;
    fn output_type(&self) -> StepDataType;
    fn estimate(&self, input_size: u64) -> OptimizeEstimate;
    async fn execute(&self, input: StepData, ctx: &PipelineContext) -> Result<StepData>;
}
```

Добавить EPUB = реализовать `EpubExtractor: OptimizeStep`. Конфигурируется через TOML. Composable — шаги можно комбинировать в цепочки.

### 2. EnrichmentStep — новые индексы и обогащения

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

Добавить GeoIndex = реализовать `GeoIndexBuilder: EnrichmentStep`. Регистрируется в системе и участвует в поиске.

### 3. CrossSourceStep — связи между источниками

```rust
trait CrossSourceStep: Send + Sync {
    fn name(&self) -> &str;
    fn required_enrichments(&self) -> Vec<EnrichmentKind>;
    async fn execute(&self, sources: &[&dyn SourceAccess], ctx: &PipelineContext) -> Result<()>;
}
```

### 4. Chunker — стратегии нарезки текста

```rust
trait Chunker: Send + Sync {
    fn chunk(&self, doc: &OptimizedDocument) -> Result<Vec<Chunk>>;
}
```

Стратегии определяются content_type документа:
- **SemanticChunker** — по headers, paragraphs (статьи, книги)
- **WindowChunker** — sliding window 10 msg / step 5 (чаты)
- **PageChunker** — по страницам (PDF)
- **FixedChunker** — по N символов (fallback)

Формат-агностик: статья из Wikipedia и статья из PDF чанкуются одинаково — оба markdown после OPTIMIZED.

### 5. Searcher — способы поиска

```rust
trait Searcher: Send + Sync {
    fn name(&self) -> &str;
    fn search_mode(&self) -> SearchMode;
    async fn search(&self, query: &SearchQuery) -> Result<Vec<ScoredChunk>>;
}
```

Реализации: `FtsSearcher` (Tantivy), `VectorSearcher` (LanceDB), `HybridSearcher` (RRF combo), `GeoSearcher`, `GraphSearcher`.

### 6. EmbeddingModel — модели для vectors

```rust
trait EmbeddingModel: Send + Sync {
    fn name(&self) -> &str;
    fn dimensions(&self) -> u32;
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}
```

Реализации: `OnnxModel` (multilingual-e5-small), `CandleModel`, `OllamaModel`.

### 7. LlmBackend — LLM для enrichment

```rust
trait LlmBackend: Send + Sync {
    fn name(&self) -> &str;
    async fn generate(&self, prompt: &str, options: &LlmOptions) -> Result<String>;
    fn supports_vision(&self) -> bool;
}
```

Реализации: `OllamaBackend`, `OnnxBackend`, `OpenAiCompatibleBackend`, `SkipBackend`.

### Вспомогательные traits

```rust
/// Рендеринг документа для просмотра
trait Renderer: Send + Sync {
    fn format(&self) -> OutputFormat;  // Terminal | Html | Markdown | Json
    fn render(&self, doc: &OptimizedDocument, highlight: Option<&TextSpan>) -> String;
}

/// Кеширование
trait CacheBackend: Send + Sync {
    async fn get(&self, key: &CacheKey) -> Option<CachedItem>;
    async fn put(&self, key: CacheKey, item: CachedItem);
    async fn evict_to(&self, target_size: u64);
}

/// Budget policy
trait BudgetPolicy {
    fn plan_degradation(&self, usage: &StorageUsage, budget: &Budget) -> Vec<DegradeAction>;
    fn can_afford(&self, estimate: &EnrichmentEstimate) -> BudgetDecision;
}
```

### Конфигурация

Все расширения конфигурируются через TOML:

```toml
# Optimize pipeline — per source
[[datasource.wikipedia.optimize.steps]]
step = "zim_article_extractor"

# Enrichment — global presets или custom
[enrichment]
preset = "full"  # fast | full | custom

# Custom enrichment steps
[[enrichment.steps]]
step = "custom_chunker"
plugin = "lokb-chunker-llm"    # внешний crate

# Шаги можно отключить
[[enrichment.steps]]
step = "embedding_indexer"
enabled = false

# LLM/embedding backends
[embedding]
model = "multilingual-e5-small"

[llm]
backend = "ollama"
model = "phi3"
fallback = "skip"
```

### Crate mapping

| Trait | Crate |
|---|---|
| OptimizeStep | `lokb-pipeline` (trait), `lokb-parsers` (implementations) |
| EnrichmentStep | `lokb-pipeline` (trait), `lokb-ingest` (implementations) |
| Chunker | `lokb-pipeline` (trait), `lokb-ingest` (implementations) |
| Searcher | `lokb-search` |
| EmbeddingModel | `lokb-embed` |
| LlmBackend | `lokb-llm` |
| Renderer | `lokb-render` |
| BudgetPolicy | `lokb-core` |

## Consequences

**Плюсы:**
- Добавление нового формата = 1 trait impl + TOML config
- Каждое расширение изолировано — не ломает существующий код
- Pipeline composable — шаги можно комбинировать, заменять, отключать
- Fallback на каждом шаге — LLM недоступен → skip

**Минусы:**
- Trait objects → dynamic dispatch (overhead ~ns, acceptable)
- Plugin system (внешние crates) потребует стабильного API
- Много trait-ов — learning curve для контрибьюторов

## Alternatives considered

1. **Enum вместо trait:** закрытый набор расширений, нужно менять код core для добавления формата
2. **WASM plugins:** максимальная изоляция, но overhead и сложность FFI
3. **Scripting (Lua/Rhai):** гибкость, но performance и типобезопасность
4. **Выбрали traits:** Rust-native, zero-cost абстракция, compile-time safety, в будущем можно добавить plugin loading через dylib
