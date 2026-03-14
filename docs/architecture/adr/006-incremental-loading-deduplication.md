# ADR-006: Incremental loading and deduplication

**Status:** Accepted
**Date:** 2026-03-14

## Context

Пользователь будет загружать данные многократно: обновлять Wikipedia dump каждые 3 месяца, re-export Telegram, редактировать Obsidian vault. Нужно:

- Не перерабатывать неизменённые данные (95% при типичном update)
- Корректно обрабатывать повторную загрузку одного и того же
- Не дублировать документы внутри source
- Связывать, а не дедуплицировать одинаковый контент из разных sources
- Восстанавливаться после partial failure

## Decision

### Трёхуровневая дедупликация

```
Уровень          Ключ                       Действие при дубликате
────────────────────────────────────────────────────────────────────
Source           name (unique)               reject, suggest 'update'
Document         (source_id, external_id)    skip if hash same, update if different
Cross-source     Entity Resolution           link через graph, не дедуплицировать
```

### Уровень 1: Source-level

Source name — unique key. Повторный `source add` с тем же именем → ошибка:

```
$ lokb source add wiki --raw ~/wiki.zim
Error: source 'wiki' already exists.
Use 'lokb source update wiki --raw ~/wiki.zim' to update.
```

Явное разделение `add` (создание) и `update` (обновление) предотвращает случайную перезапись.

### Уровень 2: Document-level

**Ключ дедупликации:** `(source_id, external_id)` — уникальная пара.

`external_id` — стабильный идентификатор в исходном формате:

| Источник | external_id |
|---|---|
| Wikipedia | Article title ("Paris") |
| Telegram | Message ID ("msg_12345") |
| PDF книга | ISBN или filename |
| Obsidian | Relative path ("notes/daily/2024-03-15.md") |
| Email | Message-ID header |
| Photo | Filename + EXIF datetime |
| GPS | Track filename |

**Change detection:** Blake3 content hash.

```rust
struct ChangeDetection {
    method: ChangeMethod,
}

enum ChangeMethod {
    /// Hash содержимого (default, надёжный)
    ContentHash,
    /// Timestamp файла (быстро, ненадёжно)
    FileModTime,
    /// Комбо: modtime → если изменился → hash
    ModTimeThenHash,
    /// По ID: "все сообщения с id > last_seen_id"
    IncrementalId { last_seen: String },
}
```

### Уровень 3: Cross-source

Один и тот же контент из разных sources (Paris в Wikipedia EN и Wikipedia FR, одно фото в Google Photos и Apple Photos) — **НЕ дедуплицируется**. Каждый source — изолированное пространство.

Связь между sources — через Entity Resolution (cross-source pipeline, ADR-002). Причины:
- Разные sources имеют разные metadata, SourceRef, privacy levels
- Удаление одного source не должно ломать другой
- При export public-only — personal source с тем же контентом не экспортируется

### Incremental update algorithm

```rust
struct SourceDiff {
    new_documents: Vec<RawDocument>,       // external_id нет в catalog
    changed_documents: Vec<RawDocument>,   // external_id есть, hash отличается
    unchanged_count: u64,                  // hash совпадает → skip
    deleted_documents: Vec<DocumentId>,    // в catalog есть, в новом RAW нет
    renamed_documents: Vec<(DocumentId, RawDocument)>,  // hash совпадает, external_id другой
}
```

Алгоритм:

1. Сканировать новый RAW → список `(external_id, content_hash)`
2. Загрузить текущее состояние из catalog для этого source
3. Классифицировать каждый документ: new / changed / unchanged / deleted
4. Detect renames: если content_hash удалённого == content_hash нового → rename
5. Показать отчёт пользователю
6. Выполнить: optimize new + changed, invalidate derived для changed, mark deleted

```
$ lokb source update wiki --raw ~/wiki-2024-12.zim

  Total in new dump:     6,500,000
  Unchanged (skip):      5,950,000  (91.5%)
  New articles:            250,000
  Changed articles:         45,000
  Deleted articles:          5,000

  Estimated time: 5m (vs 45m full re-import)
  Proceed? [Y/n]
```

### Каскад invalidation

При изменении документа — invalidate только его entries в каждом индексе:

```
Document content_hash changed
  → re-optimize (RAW → OPTIMIZED)
  → re-chunk (новые chunks для этого документа)
  → invalidate FTS entries
  → invalidate vector entries
  → invalidate entity mentions
  → mark cache entries stale
```

Не перестраиваем весь индекс — используем `IncrementalIndex` trait:

```rust
trait IncrementalIndex {
    async fn remove_documents(&mut self, doc_ids: &[DocumentId]) -> Result<()>;
    async fn upsert_chunks(&mut self, chunks: &[Chunk]) -> Result<()>;
}
```

### Sync strategies по типам источников

```rust
enum SyncStrategy {
    /// Одноразовый импорт (дампы)
    Once,
    /// Инкрементальный по diff
    Incremental { change_detection: ChangeMethod, deletion_policy: DeletionPolicy },
    /// File watcher (Obsidian, фото)
    FileWatch { debounce: Duration, change_detection: ChangeMethod },
    /// Полная перезагрузка (нельзя определить diff)
    FullReload,
}

enum DeletionPolicy {
    Delete,       // удалить из OPTIMIZED и DERIVED
    SoftDelete,   // пометить как удалённый, сохранить
    KeepAll,      // не удалять (только add/update)
}
```

| Источник | SyncStrategy | ChangeDetection | DeletionPolicy |
|---|---|---|---|
| Wikipedia ZIM | Once / FullReload | ContentHash | Delete |
| Wikidata JSON | Once / FullReload | ContentHash | Delete |
| PDF книги | Once | — | — |
| Obsidian vault | FileWatch(5s) | ModTimeThenHash | SoftDelete |
| Telegram export | Incremental | IncrementalId | KeepAll |
| Email MBOX | Incremental | IncrementalId | SoftDelete |
| Google Photos | Incremental | ContentHash | SoftDelete |
| GPS tracks | Once | — | — |
| Bookmarks | Incremental | ContentHash | Delete |

### Catalog schema

```sql
CREATE TABLE documents (
    id              BLOB PRIMARY KEY,
    source_id       BLOB NOT NULL,
    external_id     TEXT NOT NULL,
    content_hash    BLOB NOT NULL,
    content_size    INTEGER NOT NULL,
    indexed_at      TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'active',  -- active | deleted | stale
    UNIQUE(source_id, external_id)
);

CREATE TABLE index_versions (
    index_name      TEXT NOT NULL,
    document_id     BLOB NOT NULL,
    content_hash    BLOB NOT NULL,
    indexed_at      TEXT NOT NULL,
    PRIMARY KEY(index_name, document_id)
);

CREATE TABLE source_locks (
    source_id       BLOB PRIMARY KEY,
    locked_by       TEXT,
    locked_at       TEXT,
    expires_at      TEXT
);

CREATE TABLE optimize_checkpoints (
    source_id                   BLOB PRIMARY KEY,
    last_processed_external_id  TEXT,
    processed_count             INTEGER,
    total_count                 INTEGER,
    started_at                  TEXT
);
```

### Edge cases

**Rename detection:** файл переименован (Paris.md → Cities/Paris.md). File watcher видит Created + Deleted. Если content_hash совпадает — это rename → обновить external_id, не пересоздавать derived.

**Concurrent updates:** два `source update` для одного source → conflict. Lock per source в catalog с auto-release через 1h.

**Telegram overlap:** первый export messages 1-1000, второй 800-1500. Messages 800-1000 уже есть → skip по `(source_id, external_id)`.

**Partial failure:** optimize 950 из 1000 документов, crash. Checkpoint записан → restart → resume from checkpoint. Документы до checkpoint уже в catalog → skip.

**Chunk re-generation:** при изменении chunking strategy — все chunks пересоздаются. Tracked через `chunker_version + config_hash`. При mismatch → delete old chunks → create new. Атомарно.

## Consequences

**Плюсы:**
- 95% данных при update не перерабатываются (skip by hash)
- Чёткая модель дедупликации: strict внутри source, loose между sources
- File watcher для real-time sync (Obsidian, фото)
- Partial failure recovery через checkpoints
- Rename detection сохраняет derived data

**Минусы:**
- Blake3 hash всего документа — нужно прочитать весь файл для change detection (mitigated: ModTimeThenHash)
- Catalog (SQLite) становится critical path — нужен WAL mode и периодический backup
- Rename detection по hash может дать false positive (два разных файла с одинаковым контентом)
- Lock per source не позволяет параллельный update одного source (by design)

## Alternatives considered

1. **Content-addressable storage (like git):** дедупликация по content hash на уровне chunks — сложно, выигрыш мал (разные sources = разные chunks)
2. **Дедупликация между sources:** при export или search объединять одинаковые документы — ломает privacy model, усложняет удаление source
3. **Без incremental update (всегда full reload):** проще, но Wikipedia re-import = 45 минут вместо 5
4. **Event sourcing вместо snapshot:** хранить историю изменений — overhead для offline tool, не нужна history
