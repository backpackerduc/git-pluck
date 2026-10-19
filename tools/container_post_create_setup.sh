#!/usr/bin/env sh
# Devcontainer post-create setup (see .devcontainer/devcontainer.json).
# Installs the Python CLI tools this repo needs; add any future tool below, one line each.
uv tool install pre-commit
pre-commit install
