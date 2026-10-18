default:
    @just --list

shippable:
    python3 ./local/scripts/check-changelog.py

test: unittest lint fmt-check audit functest conttest

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

build-debug:
    cargo build
    ls -alh target/debug/constructo

build-release:
    cargo build --release
    ls -alh target/release/constructo

clear-executable-debug:
    rm -f target/debug/constructo

clear-executable-release:
    rm -f target/release/constructo

clear-executables: clear-executable-debug clear-executable-release

functest-debug:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -f target/debug/constructo target/debug/constructo-bundle
    cargo build
    bash tests/functest.sh target/debug/constructo plain

functest-setuid:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -f target/release/constructo target/release/constructo-bundle
    cargo build --release
    if [ "$(id -u)" = "0" ]; then
        chown root:root target/release/constructo
        chmod u+s target/release/constructo
    else
        sudo chown root:root target/release/constructo
        sudo chmod u+s target/release/constructo
    fi
    bash tests/functest.sh target/release/constructo setuid

functest: functest-debug functest-setuid

# Run the containerized tests. Requires podman to be installed.
conttest:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v podman >/dev/null 2>&1; then
        echo "SKIP: conttest (podman not installed)"
        exit 0
    fi
    rustup target add x86_64-unknown-linux-musl
    rm -f target/x86_64-unknown-linux-musl/debug/constructo
    cargo build --target x86_64-unknown-linux-musl
    bash tests/conttest.sh target/x86_64-unknown-linux-musl/debug/constructo

# Install constructo locally via cargo install --path . (no sudo required).
install:
    cargo install --path .

# Sign and push a release tag, triggering the release-draft.yml workflow.
# Monitor the workflow run manually via: gh run list --workflow=release-draft.yml
# Run just publish-release once CI has completed and the draft release looks correct.
# Usage: just draft-release v0.2.0
draft-release VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    # Refuse to tag a dirty working tree — uncommitted changes would not be part of the release.
    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "error: working tree has uncommitted changes; commit or stash them before tagging." >&2
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
    python3 ./local/scripts/decisions.py --init

# Add a new decision record
add-decision TOPIC:
    python3 ./local/scripts/decisions.py --add "{{TOPIC}}"
