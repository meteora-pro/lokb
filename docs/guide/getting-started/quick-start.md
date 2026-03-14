# Quick Start

## Добавить источник

```bash
# Markdown-файлы (статьи, заметки)
lokb source add my-notes --raw ~/notes/ --format markdown-dir --class personal

# Telegram export
lokb source add telegram --raw ~/tg_export/ --format telegram-export --class personal
```

## Поиск

```bash
# Текстовый поиск
lokb search "quantum computing"

# JSON формат для pipe
lokb search "Eiffel Tower" --format json | jq '.results[0]'

# Фильтр по типу
lokb search "restaurant" --personal-only
lokb search "quantum" --public-only
```

## Чтение документов

```bash
lokb read my-notes:Paris
lokb read my-notes:Paris --section "Geography"
```

## Статус хранилища

```bash
lokb storage status
lokb storage status --format json
```

## Экспорт

```bash
# Только публичные данные
lokb export knowledge.json

# Включая личные
lokb export everything.json --include-personal
```

## Список источников

```bash
lokb source list
lokb source list --format json
```
