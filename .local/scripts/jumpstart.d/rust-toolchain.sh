#!/usr/bin/env bash
# Installs the Rust toolchain and the cargo tools that the Justfile uses. Each <os>-jumpstart.sh runs it once the
# packages of the operating system, which include the C linker that rustc needs, are installed.

set -e

# rustup manages the toolchain. It is installed with the official script, which also adds ~/.cargo/bin to the PATH in
# the shell's profile files, for the shells that start after this one.
if ! command -v rustup >/dev/null 2>&1; then
    if command -v cargo >/dev/null 2>&1; then
        echo "WARNING: cargo is installed without rustup, perhaps from the system's packages; the toolchain that" \
            "rustup installs comes first on the PATH, but the other may be found by tools that do not use it." >&2
    fi
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
export PATH="$HOME/.cargo/bin:$PATH"

# The stable toolchain, with the components that `just lint` and `just fmt-check` use. Each of these does nothing that
# is not needed, so the script can be run again to bring an older installation up to date.
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
if ! rustup default >/dev/null 2>&1; then
    rustup default stable
fi

# just runs the recipes of the Justfile.
if ! command -v just >/dev/null 2>&1; then
    cargo install --locked just
fi

# cargo-audit is run by `just test`, which skips the audit, and says so, without it.
if ! command -v cargo-audit >/dev/null 2>&1; then
    cargo install --locked cargo-audit
fi

echo "The Rust toolchain is installed. If cargo is not found in a new shell, run: . \"\$HOME/.cargo/env\""
