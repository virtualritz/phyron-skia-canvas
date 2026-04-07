set shell := ["bash", "-euo", "pipefail", "-c"]

lib := justfile_directory() / "lib" / "skia.node"

# Default: build dev binary
default: build

[private]
ensure-deps:
    @test -d node_modules || npm ci --ignore-scripts

[private]
ensure-binary: ensure-deps
    @test -f {{ lib }} || npm run build -- dev

# Build native module (development)
build: ensure-deps
    npm run build -- dev

# Build optimized native module
optimized: ensure-deps
    rm -f {{ lib }}
    npm run build

# Build with custom features
dev: ensure-deps
    npm run build -- custom

# Run tests
test: ensure-binary
    node --test

# Run tests in watch mode
debug: ensure-binary
    node --test --watch

# Run visual tests
visual: ensure-binary
    node --watch-path lib --watch-path tests/visual tests/visual

# Type check with cargo
check:
    cargo check

# Remove compiled binary
clean:
    rm -f {{ lib }}

# Full clean
distclean: clean
    rm -rf node_modules
    rm -rf target/debug target/release
    cargo clean

# Print skia-safe version from Cargo.toml
skia-version:
    @grep -m 1 '^skia-safe' Cargo.toml | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?'

# Patch Cargo.toml to use local rust-skia checkout
with-local-skia:
    echo '' >> Cargo.toml
    echo '[patch.crates-io]' >> Cargo.toml
    echo 'skia-safe = { path = "../rust-skia/skia-safe" }' >> Cargo.toml
    echo 'skia-bindings = { path = "../rust-skia/skia-bindings" }' >> Cargo.toml

# Bump version, commit, tag, push, create draft release (bump: patch|minor|major)
release bump="patch":
    #!/usr/bin/env bash
    set -euo pipefail

    if [[ -n "$(git status --porcelain)" ]]; then
        echo "Error: working tree is not clean"
        exit 1
    fi

    if [[ -n "$(git cherry -v 2>/dev/null)" ]]; then
        echo "Error: unpushed commits"
        git log --oneline main --not --remotes="*/main"
        exit 1
    fi

    # bump package.json + package-lock.json
    npm version {{ bump }} --no-git-tag-version
    VERSION=$(node -p "require('./package.json').version")
    TAG="v${VERSION}"

    if gh release view "${TAG}" --json id &>/dev/null; then
        echo "Error: release ${TAG} already exists"
        git checkout -- package.json package-lock.json
        exit 1
    fi

    # bump Cargo.toml + Cargo.lock
    sed -i "0,/^version = /s/^version = \".*\"/version = \"${VERSION}\"/" Cargo.toml
    cargo update --workspace

    PRERELEASE=""
    [[ "$VERSION" == *"-rc"* ]] && PRERELEASE="--prerelease"

    echo ""
    echo "  version: ${VERSION}"
    echo "  tag:     ${TAG}"
    echo ""
    read -rp "Create release ${TAG}? [y/N] " confirm
    if [[ "$confirm" != "y" ]]; then
        echo "Aborted."
        git checkout -- package.json package-lock.json Cargo.toml Cargo.lock
        exit 1
    fi

    git add package.json package-lock.json Cargo.toml Cargo.lock
    git commit -m "${VERSION}"
    git tag -a "${TAG}" -m "${TAG}"
    git push origin main --tags
    gh release create "${TAG}" ${PRERELEASE} --draft --generate-notes

    echo ""
    echo "Draft release ${TAG} created. CI will build binaries."
    echo "When done, run: just publish"

# Undraft release and trigger npm publish
publish:
    #!/usr/bin/env bash
    set -euo pipefail

    VERSION=$(node -p "require('./package.json').version")
    TAG="v${VERSION}"

    if ! gh release view "${TAG}" --json id &>/dev/null; then
        echo "Error: release ${TAG} not found"
        exit 1
    fi

    DRAFT=$(gh release view "${TAG}" --json isDraft --jq '.isDraft')
    if [[ "$DRAFT" == "false" ]]; then
        echo "Release ${TAG} is already published."
        exit 0
    fi

    gh release edit "${TAG}" --draft=false
    echo "Release ${TAG} published on GitHub."

    gh workflow run publish.yml
    echo "NPM publish workflow triggered."
