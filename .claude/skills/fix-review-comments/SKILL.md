---
name: fix-review-comments
description: Исправить все комментарии из code review PR и ответить на них
compatibility: Requires DevBoy MCP server (mcp__dev-boy_lokb__) for GitHub integration
activation:
  - "fix review comments"
  - "fix review"
  - "fix pr comments"
  - "address review"
  - "исправь комментарии"
  - "исправь ревью"
  - "пофикси комментарии"
---

# Fix Review Comments

Исправить все замечания из code review: получить комментарии, приоритизировать, применить исправления, проверить, ответить.

## Входные данные

- `pr_number` — номер PR (например, `5` или `#5`)

**Если pr_number не указан** — определить из текущей ветки:

```bash
gh pr list --head $(git branch --show-current) --json number --jq '.[0].number'
```

## Фаза 1: Сбор комментариев

### Шаг 1.1: Получить discussions

```
mcp__dev-boy_lokb__get_merge_request_discussions({ merge_request_id: "gh#{pr_number}" })
```

### Шаг 1.2: Отфильтровать нерешённые

Оставить только комментарии, которые:
- Не resolved
- Содержат замечания (не просто "LGTM" или "ok")
- Не от автора PR (не self-comments)

### Шаг 1.3: Приоритизировать

Отсортировать по severity:
1. `[CRITICAL]` — исправить обязательно
2. `[HIGH]` — исправить
3. `[MEDIUM]` — исправить если возможно
4. `[LOW]` — по желанию, обсудить с ревьюером

### Шаг 1.4: Показать пользователю план

```
## Review Comments для PR #{pr_number}

| # | Severity | File | Comment |
|---|----------|------|---------|
| 1 | CRITICAL | src/store.rs:42 | Description... |
| 2 | HIGH | src/main.rs:15 | Description... |

Исправляю все? (y/n/выбери номера)
```

**Если пользователь выбрал конкретные номера** — исправить только их.

## Фаза 2: Исправление

### Шаг 2.1: Для каждого комментария

1. Прочитать полный файл с контекстом
2. Понять суть замечания
3. Применить исправление через Edit tool
4. **Если комментарий содержит code suggestion** — применить его (с адаптацией если нужно)
5. **Если замечание требует обсуждения** — спросить пользователя

### Шаг 2.2: Не ломать другой код

При исправлении:
- Не менять код, не связанный с комментарием
- Не рефакторить "заодно"
- Сохранять существующий стиль

## Фаза 3: Проверка

### Шаг 3.1: Запустить проверки

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

### Шаг 3.2: Исправить если что-то сломалось

Если тесты/clippy упали после исправлений — починить и повторить.

## Фаза 4: Коммит и ответы

### Шаг 4.1: Закоммитить

```bash
git add {changed_files}
git commit -m "fix(review): address PR #{pr_number} review comments

{list_of_fixed_items}

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
git push
```

### Шаг 4.2: Ответить на каждый комментарий

Для каждого исправленного комментария:

```
mcp__dev-boy_lokb__create_merge_request_comment({
  merge_request_id: "gh#{pr_number}",
  body: "✅ Fixed in commit {short_sha}"
})
```

Для комментариев, которые не исправлены (LOW или дискуссионные):

```
mcp__dev-boy_lokb__create_merge_request_comment({
  merge_request_id: "gh#{pr_number}",
  body: "ℹ️ {reason_why_not_fixed_or_discussion}"
})
```

### Шаг 4.3: Сообщить результат

```
## Исправления для PR #{pr_number}

✅ Исправлено: {count}
ℹ️ Обсуждается: {count}
⏭️ Пропущено (LOW): {count}

Commit: {sha}
```
