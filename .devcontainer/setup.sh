#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname -- "$0")/.."
rustup component add rustfmt clippy
cargo fetch --locked
npm ci --prefix tooling --no-audit --no-fund

printf '\nHatch development environment is ready.\n\n'
printf '  Build:  cargo build --locked\n'
printf '  Test:   cargo test --locked\n'
printf '          python3 -m unittest discover -s tests -v\n\n'
printf 'Run Hatch on the Docker host for live container discovery.\n'
