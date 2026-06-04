# Final Compatibility Report

This Milestone 70 snapshot records Webby's release-demo and compatibility
triage surface. The detailed,
continuously maintained capability matrix remains in
[`COMPATIBILITY.md`](COMPATIBILITY.md), with known gaps in
[`LIMITATIONS.md`](LIMITATIONS.md).

## Validated Demo Surface

- Local and HTTP URL resolution, including search input fallback.
- HTML parsing with deterministic recovery for the checked-in malformed corpus.
- CSS parsing, selectors, external stylesheets, cascade, and common visual
  properties.
- Block, inline, positioned, flex, and table layout within the documented
  subset.
- Display-list generation and deterministic software rendering to PPM.
- Font rasterization, decoded images, missing-image fallback, links, GET forms,
  constrained JavaScript basics, cookies, cache state, tabs, keyboard support,
  and debug inspection.
- Native shell composition without Chromium, WebKit, Blink, CEF, Electron,
  Gecko, Servo, or a platform WebView.

## Deterministic Evidence

- `cargo test --workspace`
- `scripts/demo.sh`
- [`../tests/fixtures/expected/corpus-compatibility-report.txt`](../tests/fixtures/expected/corpus-compatibility-report.txt)
- [`demo/color-block.ppm`](demo/color-block.ppm)
- `target/webby-real-site-smoke-m70/report.tsv` from the optional June 4, 2026
  real-site smoke run, when available locally

The checked-in fixtures are reduced local mini-sites, not copies of live pages.
Optional network-dependent smoke testing is documented in
[`REAL_SITE_SMOKE.md`](REAL_SITE_SMOKE.md).

## Milestone 70 Triage Result

The reduced local corpus is the compatibility gate: 7/7 cases load, 19/19
rendered-text checks pass, 8/8 interaction checks pass, and 7/7 deterministic
PPM hashes match. The optional real-site smoke run succeeded for diagnostics and
PPM rendering on 5/5 downloaded snapshots; `example.com` failed DNS resolution
in both Webby and `curl`.

The next evidence-backed work should focus on remote/snapshot resource
coordination, high-frequency CSS diagnostics, and JavaScript DOM/Web API gaps
that recur in smoke output. These should become reduced local fixtures before
engine changes are accepted.

## Intentional Limits

Webby is not a production browser. It does not provide complete HTML5 recovery,
modern CSS, browser-grade JavaScript, asynchronous networking, multi-process
isolation, or production security hardening. Unsupported behavior must remain
documented rather than silently approximated by fixture-specific code.
