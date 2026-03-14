---
name: solve-issue
description: Полный цикл решения задачи — от анализа issue до создания PR
compatibility: Requires DevBoy MCP server (mcp__dev-boy_lokb__) for GitHub integration
activation:
  - "solve issue"
  - "solve"
  - "implement issue"
  - "work on issue"
  - "реши задачу"
  - "возьми задачу"
  - "реализуй"
---

# Solve Issue

Полный цикл решения GitHub issue: анализ → ветка → исследование → реализация → тесты → PR.

## Входные данные

- `issue_id` — номер issue (например, `6` или `#6`)

**Если issue_id не указан** — спросить у пользователя.

## Фаза 1: Получение и анализ задачи

### Шаг 1.1: Получить информацию о задаче

```
mcp__dev-boy_lokb__get_issue({ issueKey: "gh#{issue_id}" })
```

**Если задача не найдена** — сообщить об ошибке и прекратить выполнение.

### Шаг 1.2: Определить тип задачи

По labels и title определить тип:

| Label/Keyword | Тип | Branch prefix |
|---|---|---|
| `feature`, `phase-*` | Feature | `feat/` |
| `bug`, `fix` | Bug fix | `fix/` |
| `documentation`, `docs` | Documentation | `docs/` |
| `refactor`, `architecture` | Refactoring | `refactor/` |
| `testing`, `e2e`, `bdd` | Testing | `test/` |
| `dx`, `devops`, `ci` | DevOps/DX | `chore/` |
| Остальное | Task | `feat/` |

### Шаг 1.3: Показать сводку пользователю

Вывести краткую сводку и спросить подтверждение:

```
## Задача #{issue_id}: {title}

**Тип:** {type}
**Labels:** {labels}
**Описание:** {description_summary}

Ветка: `{prefix}/{issue_id}-{slug}`

Начинаю работу? (y/n)
```

## Фаза 2: Подготовка ветки

### Шаг 2.1: Создать ветку

```bash
git checkout main
git pull origin main
git checkout -b {prefix}/{issue_id}-{slug}
```

`{slug}` — первые 3-5 слов из title в kebab-case (только ASCII, max 40 символов).

## Фаза 3: Исследование

### Шаг 3.1: Изучить архитектуру

1. Прочитать `CLAUDE.md` для понимания структуры проекта
2. Прочитать `README.md` если задача касается архитектуры
3. Найти связанные файлы через Glob/Grep по ключевым словам из задачи

### Шаг 3.2: Изучить существующий код

1. Найти файлы, которые нужно изменить
2. Прочитать их полностью
3. Понять зависимости и interfaces

### Шаг 3.3: Уточнить требования

**Если что-то неясно в задаче** — спросить у пользователя перед началом реализации. Лучше уточнить заранее, чем переделывать.

## Фаза 4: Реализация

### Шаг 4.1: Реализовать решение

1. Вносить изменения инкрементально
2. Следовать code style проекта (Rust: `cargo fmt`, `cargo clippy`)
3. Не создавать файлы без необходимости — предпочитать редактирование существующих

### Шаг 4.2: Написать/обновить тесты

1. Добавить unit тесты для новой логики
2. Добавить/обновить E2E Gherkin-сценарии если задача затрагивает CLI
3. Feature файлы на английском языке

## Фаза 5: Проверка

### Шаг 5.1: Запустить проверки

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo test --test e2e -p lokb-cli
```

### Шаг 5.2: Исправить проблемы

Если какая-то проверка не прошла — исправить и повторить Шаг 5.1.

## Фаза 6: Коммит и PR

### Шаг 6.1: Закоммитить

```bash
git add {changed_files}
git commit -m "{type}({scope}): {description}

{body}

Closes #{issue_id}

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

Формат коммита — Conventional Commits:
- `feat(scope): ...` — новая функциональность
- `fix(scope): ...` — исправление бага
- `docs: ...` — документация
- `refactor(scope): ...` — рефакторинг
- `test: ...` — тесты
- `chore: ...` — инфраструктура

### Шаг 6.2: Запушить и создать PR

```bash
git push -u origin {branch_name}
```

Создать PR через `gh pr create`:

```bash
gh pr create --title "{type}({scope}): {short_description}" --body "$(cat <<'EOF'
## Summary
{bullet_points_of_changes}

## Test plan
{checklist_of_tests}

Closes #{issue_id}

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

### Шаг 6.3: Сообщить результат

Вывести ссылку на PR и краткую сводку изменений.
