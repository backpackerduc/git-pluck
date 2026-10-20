#!/usr/bin/env sh
# Devcontainer post-create setup (see .devcontainer/devcontainer.json).

echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc && source ~/.bashrc
export PATH="$HOME/.local/bin:$PATH"

# Python
uv tool install pre-commit
pre-commit install
