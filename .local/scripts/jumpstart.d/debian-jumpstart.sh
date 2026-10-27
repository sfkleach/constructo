#!/usr/bin/env bash
# Installation script for setting up the development environment on Debian, Ubuntu and similar distributions.

set -e

# Update and install essential packages.
sudo apt update
sudo apt install -y wkhtmltopdf curl git

# Install uv, the Python package and project manager.
if ! command -v uv >/dev/null 2>&1; then
    curl -LsSf https://astral.sh/uv/install.sh | sh
    export PATH="$HOME/.local/bin:$PATH"
fi

# Install the Python version required by the syntax project.
uv python install 3.12
