# Contributing to lokb

Thank you for your interest in contributing to lokb!

## Quick Start

```bash
git clone https://github.com/meteora-pro/lokb.git
cd lokb
cargo build
cargo test
```

Or use the DevContainer (VS Code → "Reopen in Container").

## Prerequisites

- **Rust** 1.85+ (stable, edition 2024)
- **cmake**, **protobuf-compiler**, **libclang-dev**

macOS: `brew install cmake protobuf llvm`
Ubuntu: `sudo apt install cmake protobuf-compiler libprotobuf-dev libclang-dev libssl-dev pkg-config`

## Development Workflow

1. Find or create an issue in [GitHub Issues](https://github.com/meteora-pro/lokb/issues)
2. Create a branch: `feat/{issue}-{slug}` or `fix/{issue}-{slug}`
3. Implement changes
4. Verify:
   ```bash
   cargo fmt --all --check
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   ```
5. Commit using [Conventional Commits](https://www.conventionalcommits.org/):
   ```
   feat(scope): description (#issue)
   fix(scope): description (#issue)
   docs: description
   test: description
   chore: description
   ```
6. Create a PR

## PR Checklist

- [ ] `cargo fmt --all --check` — clean
- [ ] `cargo clippy --workspace -- -D warnings` — clean
- [ ] `cargo test --workspace` — passes
- [ ] E2E tests updated if CLI is affected
- [ ] Commit message follows Conventional Commits
- [ ] PR description explains what and why

## Code Style

- **Rust**: `cargo fmt` + `cargo clippy`
- **Tests**: English (feature files, step definitions, test names)
- **Code**: variable, function, and type names in English
- **Comments**: Russian with English technical terms

## Testing

E2E tests use [cucumber-rs](https://github.com/cucumber-rs/cucumber) with Gherkin scenarios:

```bash
cargo test --test e2e -p lokb-cli
```

See [Testing Guide](https://meteora-pro.github.io/lokb/en/development/testing) for details on adding new tests.

## Documentation

- **CLAUDE.md** — development reference for AI agents and developers
- **docs/** — Rspress documentation site ([guide](https://meteora-pro.github.io/lokb/))
- **docs/architecture/adr/** — Architecture Decision Records

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
