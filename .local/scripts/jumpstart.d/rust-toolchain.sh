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

# The tools are built with the stable toolchain through rustup, whichever cargo comes first on the PATH.
# just runs the recipes of the Justfile.
if ! command -v just >/dev/null 2>&1; then
    rustup run stable cargo install --locked just
fi

# cargo-audit is run by `just test`, which skips the audit, and says so, without it.
if ! command -v cargo-audit >/dev/null 2>&1; then
    rustup run stable cargo install --locked cargo-audit
fi

# The Justfile runs plain cargo, which must be rustup's proxy for the toolchain above to be used. A rustup that was not
# installed by its own script may not have put its proxies on the PATH: Homebrew's, for one, keeps them out of it, and
# then cargo may be another toolchain, such as Homebrew's rust. Only a proxy accepts a +toolchain argument.
for tool in cargo rustc; do
    if ! "$tool" +stable --version >/dev/null 2>&1; then
        echo "ERROR: $tool on the PATH ($(command -v "$tool" || echo "not found")) is not rustup's proxy, so the" \
            "Justfile would not use the stable toolchain that this script installed. Put the directory of rustup's" \
            "proxies first on the PATH (for Homebrew's rustup, \$(brew --prefix rustup)/bin), or remove the other" \
            "toolchain, and run this script again." >&2
        exit 1
    fi
done

echo "The Rust toolchain is installed. If cargo is not found in a new shell, run: . \"\$HOME/.cargo/env\""
