# Installation

## Prerequisites

- **Rust** 1.85+ (stable, edition 2024)
- **cmake**, **protobuf-compiler**, **libclang-dev** — for native dependencies

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

The binary will be at `target/release/lokb`.

## DevContainer

The project includes `.devcontainer/` for VS Code / GitHub Codespaces with a fully configured environment:

```bash
# VS Code → "Reopen in Container"
# Or GitHub Codespaces → "Create codespace"
```

Includes: Rust toolchain, cmake, protobuf, cargo-watch, cargo-nextest, Node.js, Claude Code.
