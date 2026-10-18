default:
    @just --list

shippable:
    python3 ./.local/scripts/check-changelog.py

test: unittest lint fmt-check audit functest

unittest:
    cargo test

lint:
    cargo clippy --all-targets -- -D warnings

audit:
    @echo "Running security audit..."
    @if command -v cargo-audit >/dev/null 2>&1; then \
        cargo audit; \
    else \
        echo "cargo-audit not found, skipping security audit"; \
        echo "To install cargo-audit, run: just install-audit"; \
    fi

# Install cargo-audit
install-audit:
    cargo install cargo-audit

fmt:
    cargo fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt --check

build:
    cargo build
    ls -alh _build/target/debug/constructo

build-release:
    cargo build --release
    ls -alh _build/target/release/constructo

clear-executable:
    rm -f _build/target/debug/constructo

clear-executable-release:
    rm -f _build/target/release/constructo

clear-executables: clear-executable clear-executable-release

functest:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -f _build/target/debug/constructo
    cargo build
    bash tests/functest.sh _build/target/debug/constructo plain


# Install constructo locally via cargo install --path . (no sudo required).
install:
    cargo install --path .

# Sign and push a release tag, triggering the release-draft.yml workflow.
# Monitor the workflow run manually via: gh run list --workflow=release-draft.yml
# Run just publish-release once CI has completed and the draft release looks correct.
# Usage: just draft-release v0.2.0
draft-release VERSION: test shippable
    #!/usr/bin/env bash
    set -euo pipefail
    # Refuse to tag a dirty working tree — uncommitted changes would not be part of the release.
    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "error: working tree has uncommitted changes; commit or stash them before tagging." >&2
        exit 1
    fi
    # Releases are cut from main, and only once main matches the remote.
    branch=$(git rev-parse --abbrev-ref HEAD)
    if [[ "$branch" != "main" ]]; then
        echo "error: releases must be tagged from main, but the current branch is '$branch'." >&2
        exit 1
    fi
    git fetch origin main
    if [[ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]]; then
        echo "error: local main differs from origin/main; pull or push before tagging." >&2
        exit 1
    fi
    # The tag must agree with the version that will be published.
    cargo_version=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n 1)
    if [[ "{{VERSION}}" != "v$cargo_version" ]]; then
        echo "error: tag '{{VERSION}}' does not match the Cargo.toml version 'v$cargo_version'." >&2
        exit 1
    fi
    echo "Signing and pushing tag {{VERSION}}..."
    git tag -s "{{VERSION}}" -m "Release {{VERSION}}"
    git push origin "{{VERSION}}"
    echo "Tag pushed. Monitor CI at: https://github.com/sfkleach/constructo/actions"
    echo "Run 'just publish-release {{VERSION}}' once the workflow completes successfully."

# Publish a release: triggers the release-publish.yml workflow on GitHub Actions,
# which runs cargo publish via trusted publisher and flips the draft to published.
# Only stable tags (vX.Y.Z with no suffix) are accepted.
# Run this after draft-release and once the draft release looks correct.
# Usage: just publish-release v0.2.0
publish-release VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    # Only stable tags are published — the workflow enforces this too, but fail fast locally.
    if [[ "{{VERSION}}" == *"-"* ]]; then
        echo "error: '{{VERSION}}' is not a stable tag — only vX.Y.Z tags can be published." >&2
        exit 1
    fi
    echo "Triggering release-publish workflow for {{VERSION}}..."
    gh workflow run release-publish.yml \
        --repo sfkleach/constructo \
        --field tag="{{VERSION}}"
    echo "Workflow triggered. Monitor at: https://github.com/sfkleach/constructo/actions"

# Initialize decision records
init-decisions:
    python3 ./.local/scripts/decisions.py --init

# Add a new decision record
add-decision TOPIC:
    python3 ./.local/scripts/decisions.py --add "{{TOPIC}}"
