# Installation

## Prerequisites

- **Rust** 1.85+ (stable)
- **cmake**, **protobuf-compiler**, **libclang-dev** — для нативных зависимостей

### macOS

```bash
brew install cmake protobuf llvm
```

### Ubuntu/Debian

```bash
sudo apt install cmake protobuf-compiler libprotobuf-dev libclang-dev libssl-dev pkg-config
```

## Build from source

```bash
git clone https://github.com/meteora-pro/lokb.git
cd lokb
cargo build --release
```

Бинарник будет в `target/release/lokb`.

## DevContainer

Проект включает `.devcontainer/` для VS Code / GitHub Codespaces с полностью настроенным окружением:

```bash
# Открыть в VS Code → "Reopen in Container"
# Или в GitHub Codespaces → "Create codespace"
```

Включает: Rust toolchain, cmake, protobuf, cargo-watch, cargo-nextest, Node.js, Claude Code.
