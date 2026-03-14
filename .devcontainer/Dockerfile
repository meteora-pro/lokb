FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Системные зависимости для нативных crates:
# - cmake, protobuf-compiler: lancedb (Arrow/Lance)
# - libssl-dev, pkg-config: общие нативные зависимости
# - libsqlite3-dev: rusqlite (fallback, основной режим — bundled)
# - libclang-dev: bindgen (ort, lancedb)
# - libzstd-dev: zstd (fallback, основной режим — bundled)
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
        cmake \
        protobuf-compiler \
        libprotobuf-dev \
        pkg-config \
        libssl-dev \
        libclang-dev \
        libzstd-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# cargo-binstall для быстрой установки cargo-инструментов
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# Полезные cargo-инструменты
RUN cargo binstall -y \
    cargo-watch \
    cargo-nextest \
    cargo-deny
