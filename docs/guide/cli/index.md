# CLI Reference

## Source Management

### `lokb source add`

Добавить новый источник данных.

```bash
lokb source add <name> --raw <path> --format <format> --class <class>
```

| Параметр | Описание | Значения |
|---|---|---|
| `name` | Имя источника | Любая строка |
| `--raw` | Путь к исходным данным | Директория или файл |
| `--format` | Формат данных | `markdown-dir`, `telegram-export` |
| `--class` | Класс данных | `public`, `personal` |

### `lokb source list`

Показать все источники.

```bash
lokb source list [--format json]
```

## Search

### `lokb search`

Поиск по всем источникам.

```bash
lokb search <query> [--format json] [--personal-only] [--public-only]
```

| Параметр | Описание |
|---|---|
| `--format json` | JSON вывод для pipe |
| `--personal-only` | Только личные источники |
| `--public-only` | Только публичные источники |

## Read

### `lokb read`

Прочитать документ.

```bash
lokb read <source>:<document_id> [--section <name>]
```

Примеры:
```bash
lokb read wikipedia-en:Paris
lokb read wikipedia-en:Paris --section "Geography"
```

## Storage

### `lokb storage status`

Статус хранилища по слоям.

```bash
lokb storage status [--format json]
```

## Export

### `lokb export`

Экспорт базы знаний. По умолчанию **без личных данных**.

```bash
lokb export <output_path> [--include-personal]
```
