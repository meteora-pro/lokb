# Contributing

## Setup

1. Install [prerequisites](/en/getting-started/#prerequisites)
2. Clone and build:

```bash
git clone https://github.com/meteora-pro/lokb.git
cd lokb
cargo build
cargo test
```

Or use the DevContainer (VS Code → "Reopen in Container").

## Workflow

1. Find or create an issue in [GitHub Issues](https://github.com/meteora-pro/lokb/issues)
2. Create a branch: `feat/{issue}-{slug}` or `fix/{issue}-{slug}`
3. Implement changes
4. Verify (see [Development](/en/development/) for all commands):
   ```bash
   cargo fmt --all --check
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   ```
5. Commit using Conventional Commits: `feat(scope): description (#issue)`
6. Create a PR

## PR checklist

- [ ] `cargo fmt --all --check` — clean
- [ ] `cargo clippy --workspace -- -D warnings` — clean
- [ ] `cargo test --workspace` — passes
- [ ] E2E tests updated if CLI is affected
- [ ] Commit message follows Conventional Commits
- [ ] PR description explains what and why
