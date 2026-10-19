#!/usr/bin/env sh
# Devcontainer post-create setup (see .devcontainer/devcontainer.json).
# Installs the Python CLI tools this repo needs; add any future tool below, one line each.
uv tool install pre-commit
pre-commit install

# Link Claude memory into persistent storage so it survives container rebuilds.
mkdir -p /home/ubuntu/.claude/projects/-workspace
ln -sfn /workspace/.claude-memory /home/ubuntu/.claude/projects/-workspace/memory
