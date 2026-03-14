# Testing

## E2E Tests (Gherkin/BDD)

The project uses [cucumber-rs](https://github.com/cucumber-rs/cucumber) for BDD testing.

### Running tests

```bash
cargo test --test e2e -p lokb-cli
```

### Structure

```
tests/
├── features/                  # Gherkin .feature files
│   ├── wikipedia_import.feature
│   ├── personal_data.feature
│   ├── privacy_export.feature
│   └── storage.feature
├── fixtures/                  # Test data
│   ├── wikipedia/             # 5 markdown articles
│   └── telegram/              # Synthetic chat export
└── (step definitions in crates/lokb-cli/tests/e2e.rs)
```

### Isolation

Each scenario:
- Creates its own temp directory (`tempfile::TempDir`)
- Sets `LOKB_DATA_DIR` to that directory
- Calls the compiled `lokb` binary via `std::process::Command`

### Adding a new test

1. Create/update a `.feature` file in `tests/features/`
2. Add step definitions in `crates/lokb-cli/tests/e2e.rs`
3. Add fixtures in `tests/fixtures/` if new test data is needed

### Example scenario

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
