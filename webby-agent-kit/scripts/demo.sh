#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/target/webby-demo"

mkdir -p "$OUT"

cd "$ROOT"

cargo run -q -p webby_cli -- --resolve-input "webby browser engine" > "$OUT/search-resolution.txt"
cargo run -q -p webby_cli -- --dump-dom tests/fixtures/sites/static-blog/index.html > "$OUT/static-blog.dom.txt"
cargo run -q -p webby_cli -- --dump-style tests/fixtures/sites/static-blog/index.html > "$OUT/static-blog.style.txt"
cargo run -q -p webby_cli -- --dump-layout tests/fixtures/sites/form-search/index.html --viewport-width 360 > "$OUT/form-search.layout.txt"
cargo run -q -p webby_cli -- --dump-layout tests/fixtures/sites/layout-showcase/index.html --viewport-width 360 > "$OUT/layout-showcase.layout.txt"
cargo run -q -p webby_cli -- --dump-display-list tests/fixtures/sites/image-gallery/index.html --viewport-width 360 > "$OUT/image-gallery.display-list.txt"
cargo run -q -p webby_cli -- --dump-diagnostics tests/fixtures/sites/js-todo/index.html --viewport-width 360 > "$OUT/js-todo.diagnostics.txt"
cargo run -q -p webby_cli -- --dump-diagnostics tests/fixtures/regressions/external-diagnostics.html --viewport-width 240 > "$OUT/external-diagnostics.txt"
cargo run -q -p webby_cli -- --render-ppm tests/fixtures/sites/static-blog/index.html --viewport-width 360 --output "$OUT/static-blog.ppm"
cargo run -q -p webby_cli -- --render-ppm tests/fixtures/sites/image-gallery/index.html --viewport-width 360 --output "$OUT/image-gallery.ppm"

cat > "$OUT/native-shell-walkthrough.txt" <<'EOF'
Run cargo run -p webby_app for the interactive shell walkthrough.
Open the static blog and follow its About link to demonstrate navigation.
Enter a search phrase in the address bar to demonstrate search resolution.
Open form-search/index.html, submit a query, and inspect the resulting URL.
Open image-gallery/index.html to inspect decoded images and fallback boxes.
Open js-todo/index.html and use its button to demonstrate JavaScript basics.
Use Ctrl/Command+T and Ctrl/Command+Tab to demonstrate independent tabs.
EOF

printf 'Webby demo artifacts written to %s\n' "$OUT"
