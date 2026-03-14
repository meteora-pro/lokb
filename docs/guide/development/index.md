# Development

## Build & Test

```bash
cargo build                     # сборка workspace
cargo test                      # все тесты
cargo test --test e2e -p lokb-cli  # E2E Gherkin-тесты
cargo clippy --workspace -- -D warnings  # линтер
cargo fmt --all                 # форматирование
cargo run -p lokb-cli -- <cmd>  # запуск CLI
```

## Conventions

### Conventional Commits

```
feat(scope): description     # новая функциональность
fix(scope): description      # исправление бага
docs: description             # документация
refactor(scope): description  # рефакторинг
test: description             # тесты
chore: description            # инфраструктура
```

### Branch naming

```
feat/{issue}-{slug}       # feature
fix/{issue}-{slug}        # bug fix
docs/{slug}               # documentation
refactor/{slug}           # refactoring
test/{slug}               # tests
chore/{slug}              # infrastructure
```

### Code style

- `cargo fmt` — обязателен
- `cargo clippy -D warnings` — без предупреждений
- Тесты на английском языке
- Комментарии в коде на русском, с английскими терминами
- Названия переменных, функций, типов — на английском

## Claude Code Skills

Проект использует Claude Code skills для автоматизации:

| Skill | Команда | Описание |
|---|---|---|
| solve-issue | `/solve-issue <id>` | Полный цикл: issue → ветка → реализация → PR |
| review-pr | `/review-pr <id>` | Code review с inline комментариями |
| fix-review-comments | `/fix-review-comments <id>` | Исправить замечания из review |
