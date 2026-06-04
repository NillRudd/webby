# Webby Validation

Use this file as the canonical validation checklist. Do not duplicate these
commands in milestone docs; reference this file instead.

## Standard Commands

Run these before finishing implementation, review, refactor, or docs cleanup
work:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
rg 'unwrap\(|expect\(|panic!|todo!|unimplemented!' crates
```

`rg` exits with status `1` when it finds no matches. For this command, no
matches is the expected successful result.

## Targeted Commands

For changes scoped to one or two crates, run focused tests first:

```bash
cargo test -p webby_url
cargo test -p webby_html
cargo test -p webby_css
cargo test -p webby_style
cargo test -p webby_layout
cargo test -p webby_render
cargo test -p webby_app
cargo test -p webby_cli
```

Use the crates touched by the change, then run the standard commands.

## CLI Smoke Checks

Useful debug commands:

```bash
cargo run -p webby_cli -- --resolve-input "example.com"
cargo run -p webby_cli -- --dump-dom examples/simple.html
cargo run -p webby_cli -- --dump-text examples/simple.html
cargo run -p webby_cli -- --dump-css examples/debug.html
cargo run -p webby_cli -- --dump-style examples/simple.html
cargo run -p webby_cli -- --dump-layout examples/simple.html --viewport-width 800
cargo run -p webby_cli -- --dump-display-list examples/simple.html --viewport-width 800
cargo run -p webby_cli -- --dump-diagnostics examples/debug.html --viewport-width 800
cargo run -p webby_cli -- --render-ppm examples/simple.html --viewport-width 800 --output /tmp/webby.ppm
cargo run -p webby_cli -- --show-config
cargo run -p webby_cli -- --list-history
cargo run -p webby_cli -- --list-bookmarks
```

## GUI Smoke Check

For app/window changes:

```bash
cargo run -p webby_app
```

Debug overlay demo:

1. Run `cargo run -p webby_app`.
2. Open `examples/debug.html` from the address bar, or use it as the configured
   homepage while demoing.
3. Press F12 to toggle the overlay.
4. Click page content to inspect the layout box under the cursor. The panel uses
   the page's existing layout tree and display list; it does not reparse or
   relayout the page.

In headless or non-interactive environments this runs a one-frame startup smoke
path instead of requiring a native window.

## Optional Real-Site Smoke Check

Network-dependent smoke testing is intentionally manual:

```bash
scripts/real-site-smoke.sh
```

See `docs/REAL_SITE_SMOKE.md`. This is not part of the required CI validation
loop.

## Documentation Sanity Checks

For docs cleanup, run:

```bash
OLD_NAME='Mo''th'
OLD_AGENT_KIT='moth-agent''-kit'
OLD_CODEX_PLAN='webby-codex''-plan'
rg "${OLD_NAME}|${OLD_AGENT_KIT}|${OLD_CODEX_PLAN}" .
rg 'Milestone 10: quality hardening|quality hardening' docs AGENTS.md README.md
find . -path './target' -prune -o -name '*.md' -print
```

Expected notes:

- The `placeholder` term is legitimate in roadmap/module docs for image
  fallback behavior.
- The stale-name check is written with split shell strings so this validation
  file does not match its own examples.
