# Спецификация интеграционных тестов для lokb

**Версия:** 1.0
**Дата:** 2026-03-14
**Статус:** В разработке

---

## 1. Обзор

Этот документ описывает конкретный набор интеграционных тестов для проверки основной функциональности lokb через CLI. Тесты используют:
- **Маленький кусок Wikipedia данных** (~10-20 статей, ~1-2 МБ)
- **Mock персональный датасорс** (синтетические чаты, заметки)
- **CLI интерфейс** для всех операций

## 2. Тестовые данные

### 2.1 Wikipedia Mini Dataset

**Содержимое:** 20 статей на темы:
- 5 статей: Geography (Paris, London, Tokyo, Moscow, New York)
- 5 статей: Science (Quantum Computing, Machine Learning, DNA, Photosynthesis, Gravity)
- 5 статей: History (World War II, Roman Empire, Renaissance, Industrial Revolution, Cold War)
- 5 статей: Technology (Internet, Smartphone, Computer, Database, Encryption)

**Формат:** Markdown (упрощённые версии Wikipedia статей)

**Размер:** ~1-2 МБ текста

**Структура:**
```
tests/fixtures/wikipedia-mini/
├── Geography/
│   ├── Paris.md
│   ├── London.md
│   ├── Tokyo.md
│   ├── Moscow.md
│   └── New_York.md
├── Science/
│   ├── Quantum_Computing.md
│   ├── Machine_Learning.md
│   ├── DNA.md
│   ├── Photosynthesis.md
│   └── Gravity.md
├── History/
│   ├── World_War_II.md
│   ├── Roman_Empire.md
│   ├── Renaissance.md
│   ├── Industrial_Revolution.md
│   └── Cold_War.md
└── Technology/
    ├── Internet.md
    ├── Smartphone.md
    ├── Computer.md
    ├── Database.md
    └── Encryption.md
```

**Метаданные (frontmatter в каждом файле):**
```markdown
---
title: "Paris"
external_id: "wiki:Paris"
language: "en"
category: "Geography"
created_at: "2024-01-01T00:00:00Z"
---

# Paris

Paris is the capital and most populous city of France...
```

### 2.2 Mock Personal DataSource

**Содержимое:**
1. **Личные заметки** (10 файлов markdown)
2. **Синтетические чаты** (3 разговора в JSON формате, Telegram-like)
3. **Метаданные фотографий** (5 записей с EXIF в JSON)

**Структура:**
```
tests/fixtures/personal-data/
├── notes/
│   ├── trip_to_paris_2024.md
│   ├── quantum_computing_research.md
│   ├── book_notes_sapiens.md
│   ├── project_ideas.md
│   ├── learning_rust.md
│   ├── tokyo_travel_plan.md
│   ├── ai_ml_resources.md
│   ├── home_renovation.md
│   ├── recipe_collection.md
│   └── workout_routine.md
├── chats/
│   ├── family_chat.json
│   ├── work_team_chat.json
│   └── friends_paris_trip.json
└── photos/
    └── photo_metadata.json
```

**Примеры:**

`notes/trip_to_paris_2024.md`:
```markdown
---
title: "Trip to Paris 2024"
created_at: "2024-03-15T10:00:00Z"
tags: ["travel", "paris", "vacation"]
---

# Trip to Paris 2024

Visited Eiffel Tower on March 16. Amazing view from the top!

Also went to Louvre Museum. Saw Mona Lisa - was smaller than expected.

Had dinner at a nice restaurant near Notre-Dame. Food was excellent.
```

`chats/friends_paris_trip.json`:
```json
{
  "conversation_id": "chat_001",
  "conversation_name": "Paris Trip Planning",
  "platform": "telegram",
  "participants": ["Alice", "Bob", "Charlie"],
  "messages": [
    {
      "id": 1,
      "from": "Alice",
      "timestamp": "2024-03-10T14:30:00Z",
      "text": "Hey, we should plan our Paris trip!"
    },
    {
      "id": 2,
      "from": "Bob",
      "timestamp": "2024-03-10T14:32:00Z",
      "text": "Yes! I've always wanted to see the Eiffel Tower"
    },
    {
      "id": 3,
      "from": "Charlie",
      "timestamp": "2024-03-10T14:35:00Z",
      "text": "We should also visit the Louvre"
    },
    {
      "id": 4,
      "from": "Alice",
      "timestamp": "2024-03-10T14:40:00Z",
      "text": "Let's go in mid-March, weather should be nice"
    }
  ]
}
```

`photos/photo_metadata.json`:
```json
[
  {
    "filename": "IMG_0001.jpg",
    "timestamp": "2024-03-16T15:30:00Z",
    "latitude": 48.858370,
    "longitude": 2.294481,
    "location_name": "Eiffel Tower",
    "camera": "iPhone 15 Pro",
    "description": "View from Eiffel Tower"
  },
  {
    "filename": "IMG_0002.jpg",
    "timestamp": "2024-03-17T11:20:00Z",
    "latitude": 48.860611,
    "longitude": 2.337644,
    "location_name": "Louvre Museum",
    "camera": "iPhone 15 Pro",
    "description": "Mona Lisa painting"
  }
]
```

---

## 3. Тестовые сценарии

### 3.1 Базовая настройка (Test Setup)

**Цель:** Инициализировать lokb и подготовить тестовое окружение

**Команды:**
```bash
# 1. Инициализация (создание директорий и конфига)
lokb init --data-dir /tmp/lokb-test

# 2. Проверка статуса
lokb status

# Expected output:
# lokb v0.1.0
# Data directory: /tmp/lokb-test
# Sources: 0
# Documents: 0
# Storage used: 0 bytes
```

**Проверки:**
- [ ] Создана структура директорий: raw/, source/, derived/, cache/
- [ ] Создан config.toml
- [ ] Команда `lokb status` возвращает успешный код (exit 0)

---

### 3.2 Добавление Wikipedia источника

**Цель:** Добавить и оптимизировать Wikipedia dataset

**Команды:**
```bash
# 1. Добавить Wikipedia как public source
lokb source add wikipedia-mini \
  --raw tests/fixtures/wikipedia-mini/ \
  --format markdown-dir \
  --class public \
  --raw-retention keep

# 2. Проверить добавление
lokb source list

# Expected output:
# ID              Type      Class    Status      Documents
# wikipedia-mini  markdown  public   raw         0

# 3. Оптимизировать (RAW → OPTIMIZED)
lokb source optimize wikipedia-mini

# 4. Проверить статус после оптимизации
lokb source status wikipedia-mini

# Expected output:
# Source: wikipedia-mini
# Class: public
# Status: optimized
# Documents: 20
# Size (RAW): ~2 MB
# Size (OPTIMIZED): ~800 KB
# Raw retention: keep
```

**Проверки:**
- [ ] Source добавлен успешно
- [ ] После оптимизации: 20 документов
- [ ] OPTIMIZED размер меньше RAW (благодаря zstd)
- [ ] Metadata извлечены (title, language, category)

---

### 3.3 Индексация Wikipedia (OPTIMIZED → DERIVED)

**Цель:** Создать индексы для поиска

**Команды:**
```bash
# 1. Индексация (chunking + FTS, без embeddings пока)
lokb ingest wikipedia-mini --skip-embeddings

# 2. Проверить статус индексации
lokb storage status

# Expected output:
# Layer         Size      Documents  Chunks
# RAW           ~2 MB     -          -
# OPTIMIZED     ~800 KB   20         -
# DERIVED       ~5 MB     20         ~150
# CACHE         0 bytes   -          -

# 3. Проверить каталог
lokb catalog query --source wikipedia-mini --limit 5

# Expected output (first 5 docs):
# doc_id                                title              category
# 01234567-89ab-cdef-0123-456789abcdef  Paris             Geography
# ...
```

**Проверки:**
- [ ] Создано ~150 chunks (примерно 7-8 chunks на статью)
- [ ] FTS индекс создан
- [ ] Catalog содержит 20 записей
- [ ] Время индексации <5 секунд

---

### 3.4 Поиск по Wikipedia (FTS)

**Цель:** Проверить keyword search

**Команды и проверки:**

**Тест 1: Простой поиск**
```bash
lokb search "Paris capital France"

# Expected output:
# 1. Paris [wikipedia-mini]
#    Paris is the capital and most populous city of France...
#    Source: tests/fixtures/wikipedia-mini/Geography/Paris.md
#
# Found 1 result in 15ms
```
- [ ] Находит статью "Paris"
- [ ] Citation содержит file path
- [ ] Latency <50ms

**Тест 2: Поиск по науке**
```bash
lokb search "quantum computing qubits"

# Expected output:
# 1. Quantum Computing [wikipedia-mini]
#    Quantum computing uses quantum-mechanical phenomena such as superposition...
#    Mentions: qubits, superposition, entanglement
#    Source: tests/fixtures/wikipedia-mini/Science/Quantum_Computing.md
```
- [ ] Находит статью "Quantum Computing"
- [ ] Релевантные фрагменты с ключевыми словами

**Тест 3: Поиск с фильтром**
```bash
lokb search "empire" --source wikipedia-mini --category History

# Expected output:
# 1. Roman Empire [wikipedia-mini]
#    The Roman Empire was the post-Republican period...
#    Category: History
```
- [ ] Находит "Roman Empire"
- [ ] Фильтр по категории работает

**Тест 4: Поиск без результатов**
```bash
lokb search "supercalifragilisticexpialidocious"

# Expected output:
# No results found.
```
- [ ] Корректно обрабатывает отсутствие результатов

---

### 3.5 Чтение документов

**Цель:** Проверить source viewer

**Команды:**
```bash
# 1. Прочитать документ по ID (взять из предыдущего search)
lokb read <doc_id>

# Expected: Полный текст статьи Paris

# 2. Прочитать по external_id
lokb read wikipedia-mini:Paris

# Expected: Полный текст статьи Paris

# 3. Прочитать с секцией (если есть headers)
lokb read wikipedia-mini:Paris --section "History"

# Expected: Только секция History
```

**Проверки:**
- [ ] Документ читается полностью
- [ ] Markdown форматирование сохранено
- [ ] Latency <500ms (cache miss), <100ms (cache hit)

---

### 3.6 Добавление персонального источника

**Цель:** Добавить personal data и проверить privacy

**Команды:**
```bash
# 1. Добавить личные заметки
lokb source add my-notes \
  --raw tests/fixtures/personal-data/notes/ \
  --format markdown-dir \
  --class personal \
  --privacy-level private

# 2. Оптимизировать
lokb source optimize my-notes

# 3. Индексировать
lokb ingest my-notes --skip-embeddings

# 4. Проверить статус
lokb source list

# Expected output:
# ID              Type      Class      Privacy   Documents
# wikipedia-mini  markdown  public     public    20
# my-notes        markdown  personal   private   10
```

**Проверки:**
- [ ] Personal source добавлен
- [ ] Privacy level = Private
- [ ] 10 заметок проиндексированы

---

### 3.7 Добавление чатов (Personal Messaging)

**Цель:** Протестировать chat parser и segmentation

**Команды:**
```bash
# 1. Добавить чаты
lokb source add my-chats \
  --raw tests/fixtures/personal-data/chats/ \
  --format telegram-json \
  --class personal \
  --privacy-level private

# 2. Настроить pipeline для чатов (в config.toml или через CLI)
# Pipeline: telegram_parser → chat_segmenter → content_store_writer

# 3. Оптимизировать
lokb source optimize my-chats

# 4. Индексировать
lokb ingest my-chats --skip-embeddings

# 5. Проверить
lokb source status my-chats

# Expected:
# Documents: 3 (conversations)
# Chunks: ~12 (sliding window по сообщениям)
```

**Проверки:**
- [ ] 3 разговора распознаны
- [ ] Chat segmentation работает
- [ ] Chunks сохраняют контекст (несколько сообщений)

---

### 3.8 Поиск в личных данных

**Цель:** Проверить поиск по personal sources

**Команды:**

**Тест 1: Поиск в заметках**
```bash
lokb search "Eiffel Tower" --source my-notes

# Expected output:
# 1. Trip to Paris 2024 [my-notes]
#    Visited Eiffel Tower on March 16. Amazing view from the top!
#    Privacy: Private
#    Source: tests/fixtures/personal-data/notes/trip_to_paris_2024.md
```
- [ ] Находит заметку о поездке
- [ ] Privacy indicator показан

**Тест 2: Поиск в чатах**
```bash
lokb search "Louvre" --source my-chats

# Expected output:
# 1. Paris Trip Planning [my-chats]
#    Charlie [2024-03-10 14:35]: We should also visit the Louvre
#    Privacy: Private
```
- [ ] Находит сообщение в чате
- [ ] Контекст разговора сохранён

**Тест 3: Поиск только в public**
```bash
lokb search "Paris" --public-only

# Expected output:
# 1. Paris [wikipedia-mini]
#    Paris is the capital and most populous city of France...
#    Class: Public
```
- [ ] Показывает только Wikipedia, НЕ личные данные

**Тест 4: Поиск только в personal**
```bash
lokb search "Paris" --personal-only

# Expected output:
# 1. Trip to Paris 2024 [my-notes]
#    ...
# 2. Paris Trip Planning [my-chats]
#    ...
```
- [ ] Показывает только personal sources

**Тест 5: Поиск по всем источникам**
```bash
lokb search "Paris"

# Expected output (mixed, но с privacy indicators):
# 1. Paris [wikipedia-mini] [Public]
#    Paris is the capital...
# 2. Trip to Paris 2024 [my-notes] [Private]
#    Visited Eiffel Tower...
# 3. Paris Trip Planning [my-chats] [Private]
#    Hey, we should plan...
```
- [ ] Показывает все результаты
- [ ] Privacy level помечен для каждого результата

---

### 3.9 Entity Linking (Personal → Public)

**Цель:** Проверить асимметричную связь Personal → Public

**Предварительно:** Нужен базовый entity extractor (простой — по exact match с названиями статей Wikipedia)

**Команды:**
```bash
# 1. Извлечь entities из personal data (упрощённо — по упоминаниям Wikipedia статей)
lokb entity extract my-notes --link-to wikipedia-mini

# 2. Проверить что личная заметка ссылается на Paris entity
lokb read my-notes:trip_to_paris_2024 --show-entities

# Expected:
# Entities mentioned:
# - Paris (Entity:wikipedia-mini:Paris)
# - Eiffel Tower (Entity:wikipedia-mini:Eiffel_Tower) [if we had this article]

# 3. Проверить что Paris entity НЕ знает о личных данных
lokb entity show wikipedia-mini:Paris --show-mentions

# Expected:
# Entity: Paris
# Type: City
# From: wikipedia-mini
#
# Mentioned in documents: 0 personal sources (privacy isolation)
```

**Проверки:**
- [ ] Personal note → Paris entity link создан
- [ ] Paris entity → personal note link НЕ создан (асимметрия)
- [ ] Privacy isolation работает

---

### 3.10 Export/Import (Privacy Test)

**Цель:** Проверить что personal data НЕ экспортируется по умолчанию

**Команды:**
```bash
# 1. Export без флага --include-personal
lokb export /tmp/knowledge-base.tar.zst

# 2. Проверить содержимое архива
tar -tzf /tmp/knowledge-base.tar.zst | head -20

# Expected:
# source/wikipedia-mini/...
# derived/...
# НЕТ: source/my-notes/, source/my-chats/

# 3. Import в новое окружение
lokb init --data-dir /tmp/lokb-test-imported
cd /tmp/lokb-test-imported
lokb import /tmp/knowledge-base.tar.zst

# 4. Проверить sources
lokb source list

# Expected:
# ID              Type      Class    Documents
# wikipedia-mini  markdown  public   20
# (my-notes и my-chats отсутствуют)

# 5. Поиск
lokb search "Paris"

# Expected: Только Wikipedia результаты, личных данных нет
```

**Проверки:**
- [ ] Export НЕ включает personal sources
- [ ] Import восстанавливает только public data
- [ ] Поиск работает в импортированной базе

---

### 3.11 Storage Management

**Цель:** Проверить команды управления хранилищем

**Команды:**
```bash
# 1. Статус хранилища
lokb storage status

# Expected output:
# Layer         Size      Limit     Usage
# RAW           ~2 MB     50 GB     0.004%
# OPTIMIZED     ~1 MB     30 GB     0.003%
# DERIVED       ~6 MB     60 GB     0.01%
# CACHE         0 bytes   15 GB     0%
# Total         ~9 MB     155 GB    0.006%

# 2. Очистка кэша
lokb cache clear

# 3. Удаление RAW после оптимизации
lokb raw delete wikipedia-mini

# 4. Проверить что RAW удалён
lokb storage status

# Expected: RAW size = 0 для wikipedia-mini
```

**Проверки:**
- [ ] Storage status показывает корректные размеры
- [ ] Cache clear работает
- [ ] RAW deletion работает (если retention = delete)

---

### 3.12 Skills (Query Templates)

**Цель:** Проверить предопределённые skills

**Команды:**

**Skill: lookup**
```bash
lokb lookup "capital of France"

# Expected (quick mode, FTS-based):
# Paris
# Source: wikipedia-mini:Paris
# "Paris is the capital and most populous city of France"
```

**Skill: define**
```bash
lokb define "quantum computing"

# Expected:
# Quantum Computing
# Quantum computing is a type of computation that uses quantum-mechanical
# phenomena such as superposition and entanglement...
# Source: wikipedia-mini:Quantum_Computing
```

**Skill: personal**
```bash
lokb personal "planning trip"

# Expected:
# 1. Paris Trip Planning [my-chats]
#    Alice: Hey, we should plan our Paris trip!
#    ...
```

**Проверки:**
- [ ] Skills работают как shortcuts
- [ ] Корректно выбирают sources
- [ ] Форматирование выводов соответствует skill config

---

## 4. Негативные тесты

### 4.1 Некорректный ввод

```bash
# 1. Несуществующий source
lokb search "test" --source nonexistent
# Expected: Error: Source 'nonexistent' not found

# 2. Несуществующий document
lokb read nonexistent:doc
# Expected: Error: Document not found

# 3. Дублирование source ID
lokb source add wikipedia-mini --raw /tmp/test
# Expected: Error: Source 'wikipedia-mini' already exists

# 4. Невалидный format
lokb source add test --raw /tmp --format invalid-format
# Expected: Error: Unknown format 'invalid-format'
```

**Проверки:**
- [ ] Корректные error messages
- [ ] Exit code != 0
- [ ] Не происходит crash

### 4.2 Пустые результаты

```bash
# Поиск по несуществующему термину
lokb search "xyzabc123nonexistent"
# Expected: "No results found."

# Поиск в пустом source (если удалили все docs)
lokb search "test" --source empty-source
# Expected: "No results found."
```

---

## 5. Тестовый скрипт (Shell)

Создать `tests/integration_test.sh`:

```bash
#!/bin/bash
set -e  # Exit on error

LOKB_BIN="./target/debug/lokb"
TEST_DIR="/tmp/lokb-integration-test"
FIXTURES_DIR="$(pwd)/tests/fixtures"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Test counter
TESTS_RUN=0
TESTS_PASSED=0

# Helper function
test_command() {
    local test_name="$1"
    shift
    echo -n "Testing: $test_name ... "
    TESTS_RUN=$((TESTS_RUN + 1))

    if "$@" > /dev/null 2>&1; then
        echo -e "${GREEN}PASS${NC}"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}FAIL${NC}"
        echo "Command failed: $@"
    fi
}

# Cleanup
rm -rf "$TEST_DIR"

# Test 1: Init
test_command "lokb init" \
    $LOKB_BIN init --data-dir "$TEST_DIR"

# Test 2: Add Wikipedia source
test_command "Add Wikipedia source" \
    $LOKB_BIN source add wikipedia-mini \
        --raw "$FIXTURES_DIR/wikipedia-mini" \
        --format markdown-dir \
        --class public

# Test 3: Optimize
test_command "Optimize Wikipedia" \
    $LOKB_BIN source optimize wikipedia-mini

# Test 4: Ingest
test_command "Ingest Wikipedia" \
    $LOKB_BIN ingest wikipedia-mini --skip-embeddings

# Test 5: Search
test_command "Search Paris" \
    $LOKB_BIN search "Paris capital France" | grep -q "Paris"

# Test 6: Add personal notes
test_command "Add personal notes" \
    $LOKB_BIN source add my-notes \
        --raw "$FIXTURES_DIR/personal-data/notes" \
        --format markdown-dir \
        --class personal

# Test 7: Optimize personal
test_command "Optimize personal notes" \
    $LOKB_BIN source optimize my-notes

# Test 8: Ingest personal
test_command "Ingest personal notes" \
    $LOKB_BIN ingest my-notes --skip-embeddings

# Test 9: Search public only
test_command "Search public only" \
    bash -c "$LOKB_BIN search 'Paris' --public-only | grep -q 'wikipedia-mini' && \
             ! $LOKB_BIN search 'Paris' --public-only | grep -q 'my-notes'"

# Test 10: Export (without personal)
test_command "Export knowledge base" \
    $LOKB_BIN export "$TEST_DIR/export.tar.zst"

# Summary
echo ""
echo "================================"
echo "Tests run: $TESTS_RUN"
echo "Tests passed: $TESTS_PASSED"
echo "Tests failed: $((TESTS_RUN - TESTS_PASSED))"
echo "================================"

if [ $TESTS_RUN -eq $TESTS_PASSED ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi
```

---

## 6. CI Integration

Добавить в `.github/workflows/integration-tests.yml`:

```yaml
name: Integration Tests

on:
  pull_request:
  push:
    branches: [main]

jobs:
  integration-tests:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rust-lang/setup-rust-toolchain@v1

      - name: Build lokb
        run: cargo build

      - name: Create test fixtures
        run: |
          mkdir -p tests/fixtures/wikipedia-mini
          # Generate test data (можно добавить скрипт генерации)

      - name: Run integration tests
        run: bash tests/integration_test.sh
```

---

## 7. Критерии успеха

Все интеграционные тесты считаются успешными, если:

- [ ] ✅ Wikipedia mini dataset успешно индексируется (20 docs)
- [ ] ✅ FTS search находит релевантные результаты (<50ms)
- [ ] ✅ Personal data sources добавляются и индексируются
- [ ] ✅ Privacy isolation работает (--public-only, --personal-only)
- [ ] ✅ Entity linking Personal → Public работает (асимметрично)
- [ ] ✅ Export НЕ включает personal data по умолчанию
- [ ] ✅ Import восстанавливает public data
- [ ] ✅ Storage management команды работают
- [ ] ✅ Skills (lookup, define, personal) работают корректно
- [ ] ✅ Негативные тесты обрабатываются gracefully
- [ ] ✅ Все тесты в `integration_test.sh` проходят

---

## 8. Следующие шаги

После прохождения базовых интеграционных тестов:

1. **Добавить embeddings тесты** (Phase 2)
   - Vector search на mini dataset
   - Hybrid search (FTS + vector)

2. **Добавить Wikidata mini** (Phase 3)
   - 50-100 entities
   - Entity resolution
   - Graph queries

3. **Расширить personal data** (Phase 4)
   - GPS tracks (GPX)
   - Photo metadata
   - Email threads

4. **Performance benchmarks**
   - Ingestion throughput
   - Search latency
   - Storage compression ratio

---

**Версия:** 1.0
**Статус:** Готово к имплементации
**Следующий шаг:** Создать test fixtures и integration_test.sh
