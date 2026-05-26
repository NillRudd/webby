# Webby Compatibility Report

This report describes the current checked-in fixture compatibility target. It
is intentionally modest: Webby is a from-scratch educational browser engine,
not a Chromium/WebKit/WebView wrapper.

## Works In Local Fixtures

- Static HTML pages with semantic structure, links, images, forms, and external
  stylesheets.
- PNG/JPEG images, data URL images, image placeholders, and video poster images.
- GET forms, URL-encoded POST forms, address-bar navigation, link clicks,
  back/forward, reload, tabs,
  local/session storage, cookies, and the deterministic resource cache.
- Inline and external classic JavaScript for DOM text/attribute mutation,
  click/input/submit events, simple timers, same-origin static-string
  `fetch()`/XHR, Web Storage, and small side-effect JavaScript module graphs.
- Basic block, inline, flex, grid, table, positioned layout, scoped responsive
  media queries, scoped pseudo-class styling, software rendering, and
  deterministic CLI dumps.
- Scoped CSS color/background transitions driven by a deterministic app clock.
- Basic iframes loaded through the normal page pipeline, with clipped rendered
  child content and link navigation inside the iframe context.

## Fixture Coverage

- `tests/fixtures/sites/static-blog/`
- `tests/fixtures/sites/image-gallery/`
- `tests/fixtures/sites/form-search/`
- `tests/fixtures/sites/layout-showcase/`
- `tests/fixtures/sites/js-todo/`
- `tests/fixtures/sites/dynamic-fetch-storage/`

The integration tests in `crates/webby_app/tests/fixture_sites.rs` load these
sites through public app/pipeline APIs and exercise links, forms, JavaScript DOM
mutation, fetch, storage, history traversal, reload, tab isolation, rendering,
and deterministic large-document behavior.

## Intentionally Unsupported

- Full JavaScript modules, import maps, live bindings, async Promise
  scheduling, dynamic fetch URL expressions, broad browser APIs, service
  workers, Web Components, and cross-frame scripting.
- Full CSS, including complete Grid and advanced responsive features beyond
  the current scoped parser/layout subset, plus keyframe animations and
  transform/opacity transitions.
- Audio/video playback, codecs, buffering, seeking, and real media controls.
- Full SVG/canvas APIs, complex SVG paths, gradients, filters, text-on-path,
  and transparent canvas compositing.
- Browser-grade networking, HTTP cache-control, process isolation, and security
  sandboxing.
- Full iframe behavior such as nested iframe loading beyond one level,
  enforced sandbox policies, `srcdoc`, and permissions/referrer policy.

When a fixture fails, the expected response is to fix the general engine
behavior or document the unsupported limitation here. Fixture-specific engine
shortcuts are not acceptable.
