# Contributing

## Setup

1. Установить [prerequisites](/getting-started/#prerequisites)
2. Клонировать и собрать:

```bash
git clone https://github.com/meteora-pro/lokb.git
cd lokb
cargo build
cargo test
```

Или использовать DevContainer (VS Code → "Reopen in Container").

## Workflow

1. Найти или создать issue в [GitHub Issues](https://github.com/meteora-pro/lokb/issues)
2. Создать ветку: `feat/{issue}-{slug}` или `fix/{issue}-{slug}`
3. Реализовать изменения
4. Проверить:
   ```bash
   cargo fmt --all --check
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   ```
5. Закоммитить с Conventional Commits: `feat(scope): description (#issue)`
6. Создать PR

## PR checklist

- [ ] `cargo fmt --all --check` — чисто
- [ ] `cargo clippy --workspace -- -D warnings` — чисто
- [ ] `cargo test --workspace` — проходит
- [ ] E2E тесты обновлены если затронут CLI
- [ ] Commit message в формате Conventional Commits
- [ ] PR description описывает что и зачем изменено

## Code style

- **Rust**: следуем `cargo fmt` + `cargo clippy`
- **Тесты**: на английском языке (Feature files, step definitions)
- **Документация**: на русском (описания, пояснения)
- **Код**: переменные, функции, типы — на английском
- **Комментарии**: на русском, с английскими техническими терминами
