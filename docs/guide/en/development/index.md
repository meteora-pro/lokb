# Development

## Build & Test

```bash
cargo build                              # build workspace
cargo test                               # all tests
cargo test --test e2e -p lokb-cli        # E2E Gherkin tests
cargo clippy --workspace -- -D warnings  # linter
cargo fmt --all                          # format
cargo run -p lokb-cli -- <cmd>           # run CLI
```

## Conventions

### Conventional Commits

```
feat(scope): description     # new feature
fix(scope): description      # bug fix
docs: description             # documentation
refactor(scope): description  # refactoring
test: description             # tests
chore: description            # infrastructure
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

- `cargo fmt` — required
- `cargo clippy -D warnings` — no warnings allowed
- Tests in English (feature files, step definitions)
- Code comments in Russian, with English technical terms
- Variable, function, and type names in English

## Claude Code Skills

The project uses Claude Code skills for development automation:

| Skill | Command | Description |
|---|---|---|
| solve-issue | `/solve-issue <id>` | Full cycle: issue → branch → implement → PR |
| review-pr | `/review-pr <id>` | Code review with inline comments |
| fix-review-comments | `/fix-review-comments <id>` | Fix review comments and reply |

See [CLAUDE.md](https://github.com/meteora-pro/lokb/blob/main/CLAUDE.md) for full development reference.
