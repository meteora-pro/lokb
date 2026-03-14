# ADR-001: Four-layer storage architecture

**Status:** Accepted
**Date:** 2026-03-14

## Context

lokb объединяет данные разного формата и размера: Wikipedia ZIM (25GB), PDF книги (15MB), Telegram JSON (500MB), фото (8MB), GPS-треки. Нужна архитектура хранения, которая:

- Разделяет "сырые" данные и производные индексы
- Позволяет удалять сырые данные после обработки (экономия места)
- Поддерживает пересоздание индексов с разной стоимостью
- Имеет понятную модель invalidation

## Decision

Четыре слоя с однонаправленным потоком данных:

```
RAW SOURCE → OPTIMIZED SOURCE → DERIVED → CACHE
```

### RAW SOURCE

Оригинальные файлы как скачаны (ZIM, PDF, JSON dump, JPG, MP4). Можно удалить после обработки.

- **Retention policies:** DeleteAfterOptimize | Keep | ExternalReference | KeepVersions(n)
- **Reacquire strategy:** Download { url, hash } | Torrent | CopyFrom | None
- **Budget:** 0-200 GB (опционально, может быть на внешнем диске)

### OPTIMIZED SOURCE

Нормализованный текст — "хранилище идей". Всё превращается в текст: фото 8MB → описание 500 bytes, видео 2GB → transcript 50KB.

- **Format:** cluster-bundle zstd (идея из Kiwix ZIM): ~1000 docs per bundle, dictionary compression
- **Неприкосновенен:** source of truth. Budget pressure не применяется к этому слою.
- **Budget:** 20-50 GB (обязательно)

### DERIVED

Множество независимых проекций данных: Chunks (LanceDB), FTS (Tantivy), Vectors (LanceDB), Entity/Relation (LanceDB+SQLite), Catalog (SQLite).

- Каждая проекция создаётся независимо
- Пересоздаётся с разной стоимостью (FTS: минуты, vectors: часы)
- Поддерживает деградацию (f32→PQ, 384→128 dims)
- **Budget:** 30-80 GB (основной потребитель — embedding vectors)

### CACHE

Рендеренные документы, распакованные bundles, query cache. LRU eviction.

- Пересоздаётся за миллисекунды
- **Budget:** 10-20 GB

## Consequences

**Плюсы:**
- RAW можно удалить после optimize → экономия 50-100 GB
- DERIVED можно пересоздать из OPTIMIZED без RAW
- Каждый индекс в DERIVED независим — можно добавлять новые без перестройки существующих
- Чёткая модель invalidation: OPTIMIZED изменился → пересоздать DERIVED

**Минусы:**
- Если RAW удалён и OPTIMIZED повреждён — данные потеряны (mitigated: checksums, reacquire strategy)
- Дублирование данных между слоями увеличивает общее потребление диска
- Сложность управления состоянием между слоями

## Alternatives considered

1. **Flat storage (один слой):** проще, но нет разделения mutable/immutable, нельзя удалить сырые данные, нельзя пересоздать индексы
2. **Two-layer (raw + index):** нет промежуточного "source of truth", пересоздание индексов требует re-parse сырых файлов
3. **Three-layer (без CACHE):** CACHE тривиален, но его явное выделение упрощает LRU eviction и budget tracking
