# Pipeline

## Composable Pipeline

Pipeline RAW→OPTIMIZED — цепочка шагов. Каждый шаг — extractor, enricher, transformer или writer:

```
RAW file → [Extract] → [Enrich via LLM] → [Clean] → [Write to Content Store]
```

## PipelineStep trait

```rust
#[async_trait]
trait PipelineStep: Send + Sync {
    fn name(&self) -> &str;
    fn input_type(&self) -> StepDataType;
    fn output_type(&self) -> StepDataType;
    async fn execute(&self, input: StepData, ctx: &PipelineContext) -> Result<StepData>;
    fn estimate_cost(&self, input_size: ByteSize) -> StepCost;
}
```

LLM-шаги опциональны с `fallback = "skip"`.

## Встроенные шаги

### Extractors (RAW → structured)

| Step | Input → Output |
|---|---|
| ZimArticleExtractor | ZIM → Markdown |
| PdfTextExtractor | PDF → Markdown |
| EpubExtractor | EPUB → Markdown |
| TelegramParser | JSON → ChatMessages |
| RdfParser | N-Triples/Turtle → Entities |
| ExifExtractor | Image → JSON |

### Enrichers (LLM/human)

| Step | Что делает | Fallback |
|---|---|---|
| ImageDescriber | Фото → текст (vision LLM) | skip |
| TextSummarizer | Текст → резюме | skip |
| EntityExtractor | Текст → NER → entities | skip |
| HumanAnnotation | Заметки от пользователя | skip |

### Transformers

| Step | Что делает |
|---|---|
| ChatSegmenter | Messages → Conversations + Threads |
| TextCleaner | Boilerplate removal, normalization |
| LanguageDetector | Detect language |

## Конфигурация (TOML)

```toml
[datasource.wikipedia-en.pipeline]

[[datasource.wikipedia-en.pipeline.steps]]
step = "zim_article_extractor"
to_markdown = true

[[datasource.wikipedia-en.pipeline.steps]]
step = "content_store_writer"
```

## Fallback стратегии

```rust
enum StepFallback {
    Skip,           // пропустить шаг, продолжить
    SkipDocument,   // пропустить документ
    Abort,          // остановить pipeline
    Retry { max_attempts: u32, delay: Duration },
}
```
