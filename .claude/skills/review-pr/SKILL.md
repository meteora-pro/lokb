---
name: review-pr
description: Code review PR — проверка diffs, pipeline, тесты, inline комментарии
compatibility: Requires DevBoy MCP server (mcp__dev-boy_lokb__) for GitHub integration
activation:
  - "review pr"
  - "review pull request"
  - "code review"
  - "проверь pr"
  - "проверь пр"
  - "ревью"
  - "сделай ревью"
---

# Review PR

Автоматизированное code review pull request: анализ diffs, проверка pipeline, inline комментарии с severity levels.

## Входные данные

- `pr_number` — номер PR (например, `5` или `#5`)

**Если pr_number не указан** — спросить у пользователя или получить список открытых PR:

```
mcp__dev-boy_lokb__get_merge_requests({ state: "opened" })
```

## Фаза 1: Сбор информации

### Шаг 1.1: Получить PR details

```
mcp__dev-boy_lokb__get_merge_request_diffs({ merge_request_id: "{pr_number}" })
mcp__dev-boy_lokb__get_merge_request_discussions({ merge_request_id: "{pr_number}" })
```

### Шаг 1.2: Проверить pipeline

```
mcp__dev-boy_lokb__get_merge_request_pipeline({ merge_request_id: "{pr_number}" })
```

**Если pipeline failed** — отметить как CRITICAL blocker в summary.

### Шаг 1.3: Прочитать связанный issue

Извлечь номер issue из описания PR (паттерн `Closes #N`). Если найден:

```
mcp__dev-boy_lokb__get_issue({ issue_id: "{issue_id}" })
```

## Фаза 2: Анализ кода

### Шаг 2.1: Проверить каждый diff по правилам

Для каждого изменённого файла проверить:

#### CRITICAL (блокирует мёрж)

- [ ] **Компиляция**: код компилируется без ошибок
- [ ] **Тесты**: новый код покрыт тестами, существующие тесты не сломаны
- [ ] **Security**: нет hardcoded секретов, нет SQL injection, нет path traversal
- [ ] **Privacy**: личные данные не утекают в public API/export
- [ ] **Pipeline**: CI проходит

#### HIGH

- [ ] **Error handling**: ошибки обрабатываются корректно, не `unwrap()` в production коде
- [ ] **unsafe**: нет необоснованного использования `unsafe`
- [ ] **Breaking changes**: API не ломается без документации

#### MEDIUM

- [ ] **Code style**: `cargo fmt` и `cargo clippy` чисто
- [ ] **Naming**: имена переменных/функций отражают смысл
- [ ] **DRY**: нет дублирования кода
- [ ] **Documentation**: публичные API имеют doc comments

#### LOW

- [ ] **Performance**: нет очевидных проблем с производительностью
- [ ] **Simplicity**: решение не переусложнено
- [ ] **Suggestions**: альтернативные подходы

### Шаг 2.2: Прочитать полные файлы при необходимости

Если diff недостаточен для оценки контекста — прочитать полный файл через Read tool.

## Фаза 3: Оставить комментарии

### Шаг 3.1: Inline комментарии

Для каждого найденного замечания оставить комментарий через MCP:

```
mcp__dev-boy_lokb__create_merge_request_comment({
  merge_request_id: "{pr_number}",
  body: "[{SEVERITY}] {description}\n\n{details_or_suggestion}"
})
```

Формат комментария:

```
**[CRITICAL]** Описание проблемы

Подробности или предложенное исправление:
\`\`\`rust
// suggested fix
\`\`\`
```

### Шаг 3.2: Не комментировать мелочи

НЕ оставлять комментарии на:
- Стилистические предпочтения (если `cargo fmt` проходит)
- Очевидный код
- Вещи не связанные с изменениями в PR

## Фаза 4: Summary

### Шаг 4.1: Написать итоговый комментарий

```
mcp__dev-boy_lokb__create_merge_request_comment({
  merge_request_id: "{pr_number}",
  body: "{summary}"
})
```

Формат summary:

```markdown
## Code Review Summary

**Verdict:** ✅ Approve / ⚠️ Request changes / ❌ Block

### Stats
- Files reviewed: {count}
- Comments: {critical} critical, {high} high, {medium} medium, {low} low

### Critical Issues
{list or "None"}

### Highlights
{what was done well}

### Suggestions
{optional improvements}
```

### Шаг 4.2: Сообщить результат пользователю

Вывести verdict и количество замечаний по severity.
