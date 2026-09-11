#!/bin/sh
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
cargo fmt --all --check
export BALATRO_FRAME_PIPELINE=2 BALATRO_ASYNC_RASTER=1
if [ -n "${BALATRO_TEST_GAME:-}" ]; then
    cargo test --locked --workspace --all-targets --all-features -- --include-ignored
else
    cargo test --locked --workspace --all-targets --all-features
fi
sh scripts/test-launcher.sh
sh scripts/test-shortcut.sh
sh scripts/test-package.sh
if command -v shellcheck >/dev/null 2>&1; then
    shellcheck -x scripts/*.sh scripts/lib/*.sh scripts/tests/*.sh port/script.sh port/Balatro.notfound
else
    echo 'ShellCheck was not found; shell linting skipped' >&2
fi
