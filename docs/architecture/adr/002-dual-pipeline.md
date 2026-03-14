# ADR-002: Dual pipeline — Optimize + Enrichment

**Status:** Accepted
**Date:** 2026-03-14

## Context

Данные проходят через два принципиально разных преобразования:

1. Извлечение сути из сырых форматов (ZIM → markdown, фото → текст)
2. Обогащение: построение индексов, связей, vectors

Эти преобразования имеют разные цели, метрики, ресурсные характеристики и должны быть независимо конфигурируемы и заменяемы.

## Decision

Три явно разделённых pipeline:

### Optimize Pipeline (RAW → OPTIMIZED)

**Цель:** сжать с потерями, извлечь суть, унифицировать формат.

**Направление:** данных становится МЕНЬШЕ.

**Метрики:**
- `compression_ratio` — вход/выход (ZIM 25GB → 4GB = 6.2x)
- `compute_time` — сколько CPU/GPU часов

**Характеристики:**
- Format-specific (каждый формат — свой набор steps)
- Потеря информации — контролируемая
- Можно повторить только если есть RAW
- LLM нужен иногда (OCR, STT, image description)

```rust
trait OptimizeStep: Send + Sync {
    fn name(&self) -> &str;
    fn estimate(&self, input_size: u64) -> OptimizeEstimate;
    async fn execute(&self, input: StepData, ctx: &PipelineContext) -> Result<StepData>;
}

struct OptimizeEstimate {
    estimated_output_size: u64,
    compression_ratio: f64,
    compute_time: Duration,
    needs_gpu: bool,
    needs_llm: Option<LlmRequirement>,
}
```

### Enrichment Pipeline (OPTIMIZED → DERIVED)

**Цель:** расширить, обогатить, построить индексы и связи.

**Направление:** данных становится БОЛЬШЕ.

**Метрики:**
- `storage_overhead` — сколько места добавляет (4GB text → 20GB derived)
- `compute_time` — сколько CPU/GPU часов

**Характеристики:**
- Format-agnostic (работает с текстом, не знает про исходный формат)
- Не теряет информацию, добавляет
- Всегда можно повторить из OPTIMIZED
- Разные steps имеют разную стоимость: FTS — минуты, vectors — часы

```rust
trait EnrichmentStep: Send + Sync {
    fn name(&self) -> &str;
    fn enrichment_kind(&self) -> EnrichmentKind;
    fn estimate(&self, chunk_count: u64) -> EnrichmentEstimate;
    async fn execute(&self, source: &dyn ChunkSource, ctx: &PipelineContext) -> Result<()>;
    fn supports_incremental(&self) -> bool;
    fn degrade_strategy(&self) -> Option<DegradeStrategy>;
}

enum EnrichmentKind {
    Chunking,
    FullTextIndex,
    EmbeddingIndex,
    EntityExtraction,
    Custom(String),
}

struct EnrichmentEstimate {
    storage_overhead: u64,
    compute_time: Duration,
    needs_gpu: bool,
    needs_llm: Option<LlmRequirement>,
    can_degrade: bool,
}
```

### Cross-Source Pipeline (DERIVED × N → unified)

**Цель:** связать данные между разными источниками.

**Когда:** после enrichment всех individual sources.

```rust
trait CrossSourceStep: Send + Sync {
    fn name(&self) -> &str;
    fn required_enrichments(&self) -> Vec<EnrichmentKind>;
    async fn execute(&self, sources: &[&dyn SourceAccess], ctx: &PipelineContext) -> Result<()>;
}
```

Примеры: EntityResolver (Paris из Wikipedia = Paris из GPS), SpatioTemporalLinker, SemanticGraphBuilder.

### Конфигурация (TOML)

```toml
# Optimize — per source, format-specific
[datasource.wikipedia-en.optimize]
[[datasource.wikipedia-en.optimize.steps]]
step = "zim_article_extractor"

# Enrichment — global or per source
[enrichment]
preset = "full"  # или custom steps

# Cross-source — global
[[cross_source.steps]]
step = "entity_resolver"
```

### Комбинирование и замена

- Шаги можно отключить (`enabled = false`)
- Шаги можно заменить (`step = "custom_chunker"`)
- Готовые presets: `fast` (chunking + FTS), `full` (+ vectors + entities), `custom`
- Fallback стратегия на каждый шаг: Skip | SkipDocument | Abort | Retry

## Consequences

**Плюсы:**
- Чёткое разделение ответственности: format parsing vs index building
- Enrichment можно запускать повторно без re-parse RAW
- Каждый enrichment step независим: можно добавить vectors позже
- Cross-source logic не усложняет individual source processing
- Метрики pipeline понятны: optimize = сжатие, enrichment = расширение

**Минусы:**
- Три pipeline → три набора конфигурации
- Cross-source steps создают implicit зависимости между sources
- Enrichment order matters: chunking → FTS/vectors → entities → cross-source

## Alternatives considered

1. **Единый pipeline (RAW → DERIVED):** проще конфигурация, но смешивает format-specific и format-agnostic логику, нельзя повторить enrichment без RAW
2. **Pipeline per index:** каждый индекс строится своим pipeline — слишком гранулярно, дублирование chunking logic
3. **Без cross-source:** entity resolution внутри individual enrichment — не имеет доступа к данным других sources
