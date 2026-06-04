#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${WEBBY_REAL_SITE_OUT:-$ROOT/target/webby-real-site-smoke}"
VIEWPORT_WIDTH="${WEBBY_REAL_SITE_VIEWPORT_WIDTH:-800}"

DEFAULT_URLS=(
  "https://example.com"
  "https://info.cern.ch"
  "https://doc.rust-lang.org/book/"
  "https://blog.rust-lang.org/"
  "https://www.rust-lang.org/"
  "https://duckduckgo.com/html/?q=webby+browser"
)

if (($# > 0)); then
  URLS=("$@")
else
  URLS=("${DEFAULT_URLS[@]}")
fi

mkdir -p "$OUT"
cd "$ROOT" || exit 1

printf 'index\turl\twebby_fetch\tcurl_snapshot\tdiagnostics\tppm\n' > "$OUT/report.tsv"
printf '%s\n' \
  "Webby real-site smoke results are optional and network-dependent." \
  "PPM output is rendered from a downloaded HTML snapshot. Linked resources resolve against the local snapshot path because the CLI does not yet expose remote page rendering." \
  "Repeated failures should become reduced local fixtures before engine changes are accepted." \
  > "$OUT/README.txt"

index=0
for url in "${URLS[@]}"; do
  index=$((index + 1))
  site="$OUT/site-$index"
  mkdir -p "$site"
  printf '%s\n' "$url" > "$site/url.txt"

  fetch_status="failure"
  if cargo run -q -p webby_cli -- --fetch "$url" > "$site/fetch.txt" 2>&1; then
    fetch_status="success"
  fi

  snapshot_status="unavailable"
  diagnostics_status="skipped"
  ppm_status="skipped"
  if command -v curl >/dev/null 2>&1; then
    if curl --location --silent --show-error --max-time 20 "$url" > "$site/snapshot.html" 2> "$site/curl.stderr.txt"; then
      snapshot_status="success"
      if cargo run -q -p webby_cli -- --dump-diagnostics "$site/snapshot.html" --viewport-width "$VIEWPORT_WIDTH" > "$site/diagnostics.txt" 2>&1; then
        diagnostics_status="success"
      else
        diagnostics_status="failure"
      fi
      if cargo run -q -p webby_cli -- --render-ppm "$site/snapshot.html" --viewport-width "$VIEWPORT_WIDTH" --output "$site/snapshot.ppm" > "$site/render.stdout.txt" 2> "$site/render.stderr.txt"; then
        ppm_status="success"
      else
        ppm_status="failure"
      fi
    else
      snapshot_status="failure"
    fi
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$index" "$url" "$fetch_status" "$snapshot_status" "$diagnostics_status" "$ppm_status" \
    >> "$OUT/report.tsv"
done

printf 'Webby optional real-site smoke artifacts written to %s\n' "$OUT"
