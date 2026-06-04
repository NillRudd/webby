# Optional Real-Site Smoke Testing

Real-site smoke testing is manual and network-dependent. It is not part of the
normal CI gate and does not claim full website compatibility.

## Run

From the repository root:

```bash
scripts/real-site-smoke.sh
```

Artifacts are written under `target/webby-real-site-smoke/`. Override the
output directory or viewport with:

```bash
WEBBY_REAL_SITE_OUT=/tmp/webby-smoke \
WEBBY_REAL_SITE_VIEWPORT_WIDTH=1024 \
scripts/real-site-smoke.sh
```

Pass URL arguments to test a smaller or different set:

```bash
scripts/real-site-smoke.sh https://example.com https://info.cern.ch
```

## Curated Targets

The default list covers:

- `https://example.com`
- `https://info.cern.ch`
- Rust documentation, blog, and static landing pages
- DuckDuckGo's HTML search result page

## Captured Results

`report.tsv` records live Webby fetch success or failure, snapshot download
status, diagnostics status, and PPM rendering status. Each numbered site
directory keeps:

- the tested URL
- Webby's live `--fetch` output
- a downloaded HTML snapshot when `curl` is available
- Webby diagnostics for that snapshot
- a PPM render attempt for that snapshot
- command stderr where relevant

The current CLI does not expose remote page rendering. PPM and diagnostics
therefore use a downloaded local HTML snapshot, and linked resources resolve
against that local snapshot path. Treat this as a useful parser/style/layout
smoke pass, not a pixel-accurate live-site screenshot.

## Findings Policy

Repeated failures should become reduced original local fixtures under
`tests/fixtures/corpus/` or `tests/fixtures/malformed/`. Fix the general owning
crate and keep network-dependent checks optional.
