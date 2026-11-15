#!/usr/bin/env bash
# Installation script for setting up the development environment on macOS.

set -e

# Homebrew is needed to install the packages below, and this script does not install it.
if ! command -v brew >/dev/null 2>&1; then
    echo "ERROR: Homebrew is required. See https://brew.sh/" >&2
    exit 1
fi

# The Xcode command line tools provide git, python3, and the C compiler and linker that rustc links with; curl comes
# with macOS.
if ! xcode-select -p >/dev/null 2>&1; then
    echo "ERROR: Install the Xcode command line tools first, with: xcode-select --install" >&2
    exit 1
fi

# The wkhtmltopdf cask has been deprecated upstream, so a failure to install it is only a warning.
if ! command -v wkhtmltopdf >/dev/null 2>&1; then
    brew install --cask wkhtmltopdf || echo "WARNING: Could not install wkhtmltopdf; PDF output will not work." >&2
fi

# Install uv, the Python package and project manager.
if ! command -v uv >/dev/null 2>&1; then
    curl -LsSf https://astral.sh/uv/install.sh | sh
    export PATH="$HOME/.local/bin:$PATH"
fi

# Install the Python version required by the syntax project.
uv python install 3.12

# Install the Rust toolchain, just and cargo-audit.
bash "$(dirname "${BASH_SOURCE[0]}")/rust-toolchain.sh"
