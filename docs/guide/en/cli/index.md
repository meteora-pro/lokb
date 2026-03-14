# CLI Reference

> Only currently implemented commands are listed below. For planned commands, see [README.md roadmap](https://github.com/meteora-pro/lokb#15-roadmap).

## Source Management

### `lokb source add`

Add a new data source.

```bash
lokb source add <name> --raw <path> --format <format> --class <class>
```

| Parameter | Description | Values |
|---|---|---|
| `name` | Source name (unique) | Any string |
| `--raw` | Path to raw data | Directory or file |
| `--format` | Data format | `markdown-dir`, `telegram-export` |
| `--class` | Data class | `public`, `personal` |

### `lokb source list`

List all configured sources.

```bash
lokb source list [--format json]
```

## Search

### `lokb search`

Search across all sources.

```bash
lokb search <query> [--format json] [--personal-only] [--public-only]
```

| Parameter | Description |
|---|---|
| `--format json` | JSON output for piping |
| `--personal-only` | Only personal sources |
| `--public-only` | Only public sources |

## Read

### `lokb read`

Read a document by reference.

```bash
lokb read <source>:<document_id> [--section <name>]
```

Examples:
```bash
lokb read my-notes:Paris
lokb read my-notes:Paris --section "Geography"
```

## Storage

### `lokb storage status`

Show storage usage by layer.

```bash
lokb storage status [--format json]
```

## Export

### `lokb export`

Export knowledge base. **Excludes personal data by default.**

```bash
lokb export <output_path> [--include-personal]
```

Currently exports a JSON manifest. Full tar.zst export is planned for Phase 2.
