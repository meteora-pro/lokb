# Testing

## E2E тесты (Gherkin/BDD)

Проект использует [cucumber-rs](https://github.com/cucumber-rs/cucumber) для BDD-тестирования.

### Запуск

```bash
cargo test --test e2e -p lokb-cli
```

### Структура

```
tests/
├── features/                  # Gherkin .feature файлы
│   ├── wikipedia_import.feature
│   ├── personal_data.feature
│   ├── privacy_export.feature
│   └── storage.feature
├── fixtures/                  # Тестовые данные
│   ├── wikipedia/             # 5 markdown-статей
│   └── telegram/              # Синтетический чат
└── (step definitions в crates/lokb-cli/tests/e2e.rs)
```

### Изоляция

Каждый сценарий:
- Создаёт свою temp-директорию (`tempfile::TempDir`)
- Устанавливает `LOKB_DATA_DIR` в эту директорию
- Вызывает скомпилированный бинарник `lokb` через `std::process::Command`

### Добавление нового теста

1. Создать/обновить `.feature` файл в `tests/features/`
2. Добавить step definitions в `crates/lokb-cli/tests/e2e.rs`
3. Добавить fixtures в `tests/fixtures/` если нужны новые данные

### Пример сценария

```gherkin
Feature: Wikipedia import and search

  Scenario: Search Wikipedia articles by keyword
    Given a clean lokb data directory
    And source "wiki" is loaded from "wikipedia/" as "markdown-dir" class "public"
    When I run lokb "search 'Eiffel Tower' --format json"
    Then the command succeeds
    And the JSON search results are not empty
    And the JSON search results contain a document with title "Paris"
```
