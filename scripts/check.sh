#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Definition-of-done gate: format, clippy, tests (offline, committed fixtures).
# The demo e2e tests need fixtures/wasm/*; regenerate with scripts/build-demos.sh.
# The golden decoder outputs come from scripts/golden.sh; the browser engine
# from scripts/build-engine.sh; the site data from `pnpm demo:data` in site/.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# clippy covers the demo crates written for this project; the copied upstream
# contracts (token, increment, oz-pausable) keep their upstream code as is.
(cd demos && cargo fmt --all -- --check \
  && cargo clippy -p ssc-demo-oz-extern -p ssc-demo-oz-keys -p ssc-capture --all-targets -- -D warnings \
  && cargo test)
# The committed engine wasm must equal a fresh build of the current sources.
bash scripts/build-engine.sh --verify
# Browser engine (site/public/engine/*.wasm, as committed) against the CLI goldens.
# Needs only Node >= 22.18 (type stripping); no site dependencies.
node site/scripts/verify-engine.mjs
echo "check.sh: OK"
