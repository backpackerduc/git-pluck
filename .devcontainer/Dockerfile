FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive

# ---------------------------------------------------------------------------
# System dependencies
#   - build-essential, pkg-config, curl: Rust / cargo basics
#   - libgit2-dev:     native binding for the `git2` crate (libgit2 ~1.7)
#   - libssl-dev:      required by libgit2 on Ubuntu 24.04
#   - git git-lfs:     needed for testing and day-to-day development
#   - ca-certificates: HTTPS requests during `cargo build`
# ---------------------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
    bash-completion \
    build-essential \
    ca-certificates \
    curl \
    git git-lfs \
    libgit2-dev \
    libssl-dev \
    pkg-config \
    sudo \
    && rm -rf /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# Rust toolchain
# ---------------------------------------------------------------------------
ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/usr/sbin:/sbin:/bin

# Install rustup itself, then explicitly install the exact toolchain.
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none && \
    rustup toolchain install 1.95.0-x86_64-unknown-linux-gnu --profile minimal && \
    rustup default 1.95.0-x86_64-unknown-linux-gnu && \
    rustup component add --toolchain 1.95.0-x86_64-unknown-linux-gnu \
        clippy \
        rust-src \
        rustfmt && \
    rustc --version && \
    cargo --version && \
    cargo clippy --version
    
# ---------------------------------------------------------------------------
# Sudo access and ownership for ubuntu user
# Ubuntu 24.04 images commonly already include an ubuntu user, so this keeps
# that user usable inside devcontainers and compose setups.
# ---------------------------------------------------------------------------
RUN echo "ubuntu ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/ubuntu && \
    chmod 0440 /etc/sudoers.d/ubuntu && \
    mkdir -p "$CARGO_HOME" "$RUSTUP_HOME" /workspace && \
    chown -R ubuntu:ubuntu "$CARGO_HOME" "$RUSTUP_HOME" /workspace

# ---------------------------------------------------------------------------
# Workspace
# ---------------------------------------------------------------------------
WORKDIR /workspace

# ---------------------------------------------------------------------------
# Entrypoint — best-effort ownership fix for standalone (docker compose) use.
#
# When the container starts as root and /workspace is mounted with files owned
# by a different UID, chown once so later non-root use works cleanly.
# ---------------------------------------------------------------------------
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["/bin/bash"]
