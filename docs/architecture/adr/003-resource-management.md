# ADR-003: Resource management — Budget, Compute Queue, Lifecycle

**Status:** Accepted
**Date:** 2026-03-14

## Context

lokb работает с ограниченными ресурсами: дисковое пространство (50-200 GB), вычислительная мощность (CPU, опционально GPU), время. Embedding 1.2M chunks занимает ~20 часов CPU. Нужна система управления ресурсами, которая:

- Не даёт превысить дисковый бюджет
- Фоново выполняет тяжёлые задачи с приоритетами
- Автоматически удаляет ненужные данные

## Decision

Три подсистемы, координируемые ResourceCoordinator:

### 1. Budget Manager — управление дисковым пространством

Иерархический бюджет по слоям и источникам:

```
Total budget
├── source/    (OPTIMIZED, неприкосновенен)
├── derived/   (деградируемый)
│   ├── per-source limits
│   └── per-index breakdown
└── cache/     (LRU, просто чистим)
```

**Ключевой принцип:** OPTIMIZED неприкосновенен. Budget pressure приходится только на DERIVED и CACHE.

**Каскад деградации** при превышении derived budget:

| Уровень | Действие | Экономия | Потеря качества |
|---|---|---|---|
| 1 | Vectors: f32 → PQ | 30x | ~5% precision |
| 2 | Vectors: 384 → 128 dims (Matryoshka) | 3x | ~10% precision |
| 3 | Удалить vectors, оставить FTS | 100% vectors | нет semantic search |
| 4 | Evict derived низкоприоритетного source | varies | нет поиска по source |

Порядок: сначала деградируем качество, потом удаляем по приоритетам. Личные данные (высокий приоритет) не трогаем, пока не деградировали публичные.

```rust
trait BudgetPolicy {
    fn plan_degradation(&self, usage: &StorageUsage, budget: &Budget) -> Vec<DegradeAction>;
    fn can_afford(&self, estimate: &EnrichmentEstimate) -> BudgetDecision;
}

enum BudgetDecision {
    Proceed,
    ProceedWithDegradation(DegradeAction),
    Defer,
    Reject { reason: String },
}
```

### 2. Task Scheduler — очередь вычислений

DAG-based очередь задач с приоритетами и зависимостями.

**Почему очередь:**
- Embedding 1.2M chunks на CPU — 20 часов, нельзя блокировать
- Пользователь добавляет source пока идёт embedding другого
- FTS готов через минуты, vectors — через часы, entity resolution — после всех sources

**Task model:**

```rust
struct ComputeTask {
    id: TaskId,
    kind: TaskKind,
    priority: u32,
    depends_on: Vec<TaskId>,
    estimate: ComputeEstimate,
    state: TaskState,
}

struct ComputeEstimate {
    cpu_time: Duration,
    needs_gpu: bool,
    memory_peak: u64,
    io_read: u64,
    io_write: u64,
}

enum TaskState {
    Queued,
    Running { progress: Progress },
    Paused,
    Completed { metrics: TaskMetrics },
    Failed { error: String, retries: u32 },
}
```

**DAG зависимостей при добавлении source:**

```
optimize → chunk → build_fts (priority: 200, доступен сразу)
                 → build_embeddings (priority: 100, фоново)
                 → extract_entities (priority: 150)
                       → resolve_entities (after all sources)
```

**Throttling:**

```toml
[compute]
max_parallel_tasks = 2
cpu_limit_percent = 80
embedding_hours = "22:00-06:00"  # или "always"
```

### 3. Garbage Collector — lifecycle данных

Три типа "ненужных" данных:

| Тип | Когда удалять | Как удалять |
|---|---|---|
| RAW после optimize | После успешного optimize + grace period 24h | По retention policy |
| DERIVED stale | Документ изменился, индекс невалиден | Incremental rebuild или full rebuild |
| CACHE | LRU eviction при превышении бюджета | Пересоздаётся за ms |
| Orphaned files | Файлы без записей в catalog | Удалить |
| Fragmented bundles | >30% пустого места в bundle | Compact |

```rust
enum CleanupPolicy {
    RawAfterOptimize { grace_period: Duration },
    CacheLru { max_size: u64 },
    DerivedStale { max_staleness: Duration },
    OrphanedFiles,
    Compaction { fragmentation_threshold: f64 },
}
```

**Invalidation tracking:**

```rust
struct InvalidationTracker {
    /// content_hash документа на момент последнего rebuild каждого индекса
    index_doc_versions: HashMap<(IndexName, DocumentId), Blake3Hash>,
}
```

Если content_hash изменился — индекс stale. Если изменилось <10% документов — incremental update, иначе full rebuild.

### ResourceCoordinator — оркестратор

Связывает Budget, Scheduler и GC. Принимает решения при добавлении source:

1. Хватает ли места? → Budget Manager
2. Если нет — можно ли освободить? → GC plan
3. Запланировать optimize → Scheduler (высокий приоритет)
4. Запланировать enrichment → Scheduler (зависит от optimize)
5. Budget позволяет vectors? → полные или degraded
6. Запланировать RAW cleanup → Scheduler (низкий приоритет)

## Consequences

**Плюсы:**
- Система не может "упасть" из-за нехватки места — graceful degradation
- Тяжёлые задачи не блокируют пользователя — FTS доступен сразу
- Автоматическая очистка ненужных данных
- Прозрачные метрики: что занимает место, что считается, когда будет готово

**Минусы:**
- Сложность реализации: три подсистемы + координатор
- Task DAG может стать сложным при многих sources
- Budget decisions могут удивить пользователя (vectors degraded без спроса)
- GC grace period может быть слишком коротким/длинным

## Alternatives considered

1. **Без budget management:** проще, но диск заполнится и всё остановится
2. **Синхронные pipeline:** проще, но embedding блокирует на 20 часов
3. **Manual cleanup:** пользователь сам решает — но забудет, и RAW 100GB будут лежать вечно
4. **Единый resource manager:** всё в одном — слишком большой объект, сложно тестировать
