#!/usr/bin/env sh
# Devcontainer post-create setup (see .devcontainer/devcontainer.json).
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc && source ~/.bashrc

# python
uv tool install pre-commit
pre-commit install

# Link Claude memory into persistent storage so it survives container rebuilds.
mkdir -p /home/ubuntu/.claude/projects/-workspace
ln -sfn /workspace/.claude-memory /home/ubuntu/.claude/projects/-workspace/memory
