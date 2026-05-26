#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/target/webby-demo"

mkdir -p "$OUT"

cd "$ROOT"

cargo run -p webby_cli -- --dump-dom tests/fixtures/sites/static-blog/index.html > "$OUT/static-blog.dom.txt"
cargo run -p webby_cli -- --dump-style tests/fixtures/sites/static-blog/index.html > "$OUT/static-blog.style.txt"
cargo run -p webby_cli -- --dump-layout tests/fixtures/sites/layout-showcase/index.html --viewport-width 360 > "$OUT/layout-showcase.layout.txt"
cargo run -p webby_cli -- --dump-display-list tests/fixtures/sites/image-gallery/index.html --viewport-width 360 > "$OUT/image-gallery.display-list.txt"
cargo run -p webby_cli -- --dump-diagnostics tests/fixtures/regressions/external-diagnostics.html --viewport-width 240 > "$OUT/external-diagnostics.txt"
cargo run -p webby_cli -- --render-ppm tests/fixtures/sites/static-blog/index.html --viewport-width 360 --output "$OUT/static-blog.ppm"
cargo run -p webby_cli -- --render-ppm tests/fixtures/sites/image-gallery/index.html --viewport-width 360 --output "$OUT/image-gallery.ppm"

printf 'Webby demo artifacts written to %s\n' "$OUT"
