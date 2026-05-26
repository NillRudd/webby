# Webby Demo Script

The demo uses only checked-in fixtures and public CLI/app commands.

## One Command

From the repository root:

```bash
scripts/demo.sh
```

The script writes deterministic demo artifacts under `target/webby-demo/`.
`docs/demo/color-block.ppm` is a small checked-in PPM reference generated from
`tests/fixtures/render/color-block.html`.

## Manual Steps

```bash
cargo test --workspace
cargo run -p webby_cli -- --dump-dom tests/fixtures/sites/static-blog/index.html
cargo run -p webby_cli -- --dump-style tests/fixtures/sites/static-blog/index.html
cargo run -p webby_cli -- --dump-layout tests/fixtures/sites/layout-showcase/index.html --viewport-width 360
cargo run -p webby_cli -- --dump-display-list tests/fixtures/sites/image-gallery/index.html --viewport-width 360
cargo run -p webby_cli -- --render-ppm tests/fixtures/sites/static-blog/index.html --viewport-width 360 --output target/webby-demo/static-blog.ppm
cargo run -p webby_cli -- --render-ppm tests/fixtures/sites/image-gallery/index.html --viewport-width 360 --output target/webby-demo/image-gallery.ppm
```

Then run:

```bash
cargo run -p webby_app
```

In a non-interactive environment, `webby_app` runs a headless startup smoke
render and exits with a clear message.
