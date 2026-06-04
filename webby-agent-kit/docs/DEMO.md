# Webby Demo

## Deterministic Bundle

Generate the local demo artifacts:

```bash
scripts/demo.sh
```

The script writes these files under `target/webby-demo/`:

- `search-resolution.txt`
- `static-blog.dom.txt`
- `static-blog.style.txt`
- `form-search.layout.txt`
- `layout-showcase.layout.txt`
- `image-gallery.display-list.txt`
- `js-todo.diagnostics.txt`
- `external-diagnostics.txt`
- `static-blog.ppm`
- `image-gallery.ppm`
- `native-shell-walkthrough.txt`

These artifacts exercise URL/search resolution, navigation-oriented DOM
inspection, CSS styling, forms, inline and block layout, images, JavaScript
diagnostics, external-resource diagnostics, display-list generation, and
software rendering. They are deterministic local fixtures and do not require
network access.

## Native Shell

Run:

```bash
cargo run -p webby_app
```

For an interactive walkthrough:

1. Open `tests/fixtures/sites/static-blog/index.html` and follow the About and
   Home links.
2. Type `webby browser engine` into the address bar and press Enter to exercise
   search resolution.
3. Open `tests/fixtures/sites/form-search/index.html`, enter a query, and submit
   the GET form.
4. Open `tests/fixtures/sites/image-gallery/index.html` to see decoded images
   and a missing-image fallback.
5. Open `tests/fixtures/sites/js-todo/index.html` and use its button to exercise
   the constrained JavaScript path.
6. Use `Ctrl/Command+T` and `Ctrl/Command+Tab` to create and switch tabs.
7. Use `F12` for debug overlay inspection and `F1` for shortcut help.

In headless environments the native adapter exits cleanly. The deterministic
CLI bundle remains available for CI and remote terminals.

## References

The checked-in render reference is [`demo/color-block.ppm`](demo/color-block.ppm).
The final release snapshot is in
[`FINAL_COMPATIBILITY_REPORT.md`](FINAL_COMPATIBILITY_REPORT.md).
