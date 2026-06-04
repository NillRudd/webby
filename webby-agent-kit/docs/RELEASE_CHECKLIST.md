# Webby Release Checklist

Use this checklist before presenting or tagging a release/demo build.

## Validation

- Run `cargo fmt --all -- --check`.
- Run `cargo clippy --workspace --all-targets -- -D warnings`.
- Run `cargo test --workspace`.
- Run `rg 'unwrap\(|expect\(|panic!|todo!|unimplemented!' crates` and confirm
  it prints no matches.

## Demo

- Run `scripts/demo.sh` from the repository root.
- Confirm the generated bundle includes search, forms, images, JavaScript
  diagnostics, style/layout dumps, display-list output, and PPM snapshots.
- Open the generated PPM files in `target/webby-demo/` with a common image
  viewer.
- Run `cargo run -p webby_app` in an interactive shell and verify the native
  startup page opens or the headless smoke path prints a clear message.
- Optionally run `scripts/real-site-smoke.sh` when network access is available.
  Review `docs/REAL_SITE_SMOKE.md` before interpreting snapshot renders.

## Documentation

- Confirm `README.md` points to the canonical docs.
- Confirm `docs/ARCHITECTURE.md` matches current crate boundaries.
- Confirm `docs/MODULES.md` lists current CLI commands and profile files.
- Confirm `docs/LIMITATIONS.md` is honest and specific.
- Confirm `docs/FINAL_COMPATIBILITY_REPORT.md` matches the release demo.
- Confirm `CONTRIBUTING.md` still points contributors to the canonical rules.

## Source Hygiene

- Do not commit generated target artifacts.
- Do not commit local profile state.
- Keep fixtures deterministic and small.
