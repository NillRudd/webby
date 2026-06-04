# Webby Compatibility Report

This report describes the current checked-in fixture compatibility target. It
is intentionally modest: Webby is a from-scratch educational browser engine,
not a Chromium/WebKit/WebView wrapper.

## Works In Local Fixtures

- Static HTML pages with semantic structure, links, images, forms, and external
  stylesheets.
- PNG/JPEG images, data URL images, image placeholders, and video poster images.
- Redirect-bounded HTTP loading, gzip response decoding, response size limits,
  and charset-aware text decoding for UTF-8 plus Latin-1-compatible pages.
- GET forms, URL-encoded POST forms, address-bar navigation, link clicks,
  keyboard link/form activation, back/forward, reload, tabs, local/session
  storage, cookies, and the deterministic resource cache.
- Inline and external classic JavaScript for DOM text/attribute mutation,
  click/input/submit events, simple timers, same-origin static-string
  `fetch()`/XHR, Web Storage, and small side-effect JavaScript module graphs.
- Open Shadow DOM v0.1 and autonomous custom elements with scoped,
  deterministic rendering behavior.
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

## Reduced Website Corpus Gate

`tests/fixtures/corpus/` contains original synthetic pages modeled after common
real-world patterns:

- documentation site
- news article page
- responsive blog
- search page
- dashboard-like app
- ecommerce product grid
- login form

The public-pipeline test `reduced_real_world_corpus_matches_compatibility_report`
loads every corpus entry locally and compares its observed compatibility report
with `tests/fixtures/expected/corpus-compatibility-report.txt`. Each report row
records required rendered-text checks, exposed interaction checks, diagnostics,
known unsupported behavior, and a deterministic rendered PPM FNV-1a hash.

## Malformed Input Recovery Gate

`tests/fixtures/malformed/` contains small original broken-input cases for HTML,
CSS, JavaScript, and missing linked resources. The loader-backed pipeline test
renders each file twice and compares diagnostics, layout dumps, and display
lists. Recovery is intentionally forgiving: valid surrounding page content
continues to render, malformed CSS and JavaScript produce deterministic
diagnostics, and missing images keep their placeholder path. The checked-in
optional-end-tag DOM dump covers paragraphs, list items, table sections, rows,
cells, escapable raw text, and generic foreign-content fallback.

Known corpus limitations are explicit:

- the ecommerce page intentionally includes one missing-image placeholder
- the login page renders and exposes controls, but has no authentication server

## Milestone 70 Scorecard

Snapshot date: June 4, 2026.

Deterministic local corpus:

- cases: 7/7 load through the public app pipeline
- rendered text checks: 19/19
- exposed interaction checks: 8/8
- unexpected diagnostics: 0
- deterministic PPM hashes: 7/7 match
- intentional unsupported cases: missing ecommerce image placeholder and login
  authentication server

Optional real-site smoke:

`WEBBY_REAL_SITE_OUT=target/webby-real-site-smoke-m70 scripts/real-site-smoke.sh`
was run against the default target list. The result was:

```text
index  url                                      webby_fetch  curl_snapshot  diagnostics  ppm
1      https://example.com                      failure      failure        skipped      skipped
2      https://info.cern.ch                     success      success        success      success
3      https://doc.rust-lang.org/book/          success      success        success      success
4      https://blog.rust-lang.org/              success      success        success      success
5      https://www.rust-lang.org/               success      success        success      success
6      https://duckduckgo.com/html/?q=webby...  success      success        success      success
```

The `example.com` failure was DNS-related and reproduced in both Webby's fetch
path and `curl`, so it is treated as network environment evidence, not an
engine regression. Snapshot diagnostics/PPM are interpreted with the limitation
documented in `docs/REAL_SITE_SMOKE.md`: downloaded single-file HTML snapshots
do not mirror all linked stylesheets, scripts, images, or fonts.

Recurring smoke findings:

- local snapshot rendering turns relative linked resources into missing local
  file diagnostics unless those resources were mirrored
- modern sites frequently use unsupported CSS custom properties, selector
  forms, filters, radius variants, and media queries
- JavaScript failures cluster around unsupported DOM/browser APIs rather than
  parser crashes
- SVG-heavy image sets render through placeholders or the current scoped SVG
  subset rather than full SVG compatibility
- network-dependent checks are useful for triage but should become reduced
  local fixtures before engine changes are accepted

Evidence-based next direction:

1. Improve remote/snapshot resource coordination before interpreting real-site
   pixels too strongly.
2. Reduce high-frequency CSS diagnostics with targeted property/value support
   only when backed by fixture cases.
3. Expand JavaScript DOM/Web API coverage from repeated smoke failures, not
   speculative API lists.
4. Keep compatibility work fixture-driven and deterministic.

## Intentionally Unsupported

- Full JavaScript modules, import maps, live bindings, async Promise
  scheduling, dynamic fetch URL expressions, broad browser APIs, service
  workers, full Web Components, and cross-frame scripting.
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
