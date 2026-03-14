# Quick Start

## Add a source

```bash
# Markdown files (articles, notes)
lokb source add my-notes --raw ~/notes/ --format markdown-dir --class personal

# Telegram export
lokb source add telegram --raw ~/tg_export/ --format telegram-export --class personal
```

Supported formats: `markdown-dir`, `telegram-export`. More formats planned (see [ADR-007](/en/architecture/#architecture-decision-records)).

## Search

```bash
# Text search
lokb search "quantum computing"

# JSON output for piping
lokb search "Eiffel Tower" --format json | jq '.results[0]'

# Filter by class
lokb search "restaurant" --personal-only
lokb search "quantum" --public-only
```

## Read documents

```bash
lokb read my-notes:Paris
lokb read my-notes:Paris --section "Geography"
```

## Storage status

```bash
lokb storage status
lokb storage status --format json
```

## Export

```bash
# Public data only (default)
lokb export knowledge.json

# Include personal data
lokb export everything.json --include-personal
```

## List sources

```bash
lokb source list
lokb source list --format json
```
