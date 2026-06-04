# Webby Fixture Sites

These fixtures exercise the implemented Webby pipeline through public app and
CLI paths. They are intentionally local and deterministic.

## Structured Groups

- `html/` - parser and visible-text fixtures.
- `css/` - selector/cascade fixtures with external stylesheets.
- `layout/` - box-model and layout dump fixtures.
- `render/` - display-list and software-render fixtures.
- `forms/` - GET form fixtures.
- `images/` - image sizing and decode fixtures.
- `navigation/` - linked local pages for URL and history behavior.
- `sites/` - complete mini-sites that cross multiple pipeline stages.
- `corpus/` - reduced synthetic pages modeled after common real-world website
  patterns. This is the main compatibility gate.
- `malformed/` - loader-backed broken-input recovery fixtures.
- `regressions/` - preserved files for previously fixed bugs.
- `expected/` - small deterministic expected-output snippets.

`fixture.toml` documents stable fixture entry points for humans and future
tooling. The current Rust tests read fixture files directly through Webby's
public APIs.

## Sites

- `sites/static-blog/` - semantic blog pages with navigation, external CSS, and
  a decoded PNG image.
- `sites/image-gallery/` - PNG/JPEG decoding, image sizing, CSS sizing, and a
  missing-image fallback.
- `sites/form-search/` - a relative GET search form and results page.
- `sites/layout-showcase/` - block and inline layout with nested links, spans,
  inline images, and an explicit note that table/flex/grid behavior is out of
  scope for the current engine.
- `sites/js-todo/` - inline JavaScript click handling and DOM mutation.
- `sites/dynamic-fetch-storage/` - same-origin `fetch()` plus local/session
  storage updates through the app pipeline.

## Reduced Compatibility Corpus

The corpus pages are original synthetic fixtures, not copied website code:

- `corpus/documentation/`
- `corpus/news-article/`
- `corpus/responsive-blog/`
- `corpus/search-page/`
- `corpus/dashboard/`
- `corpus/ecommerce-grid/`
- `corpus/login-form/`

`expected/corpus-compatibility-report.txt` records deterministic render,
interaction, diagnostic, unsupported-feature, and PPM-hash results.

## Regressions

- `regressions/raw-text-lookalikes.html`
- `regressions/malformed-selectors.html`
- `regressions/external-diagnostics.html`
- `regressions/image-decode-fallback.html`
- `regressions/form-get.html`

## Malformed Input Corpus

`malformed/` keeps deterministic recovery cases for broken HTML, CSS,
JavaScript, and linked resources. Tests render each fixture twice through the
public loader-backed pipeline and compare diagnostics and display output.

Expected-output policy lives in `expected/README.md`.
