# Start Here

This kit is for controlling Codex so it implements Webby as a serious long-running project instead of a sequence of shallow patches.

## How to install into your repo

From the root of your new `webby` repository:

```bash
cp -r path/to/webby-codex-plan/* .
cp -r path/to/webby-codex-plan/.codex . 2>/dev/null || true
```

Or unzip this package directly into the repo root.

## First Codex prompt

Use this exact prompt:

```text
Read AGENTS.md, docs/ROADMAP.md, docs/ARCHITECTURE.md, docs/QUALITY_GATES.md, and all docs/modules/*.md.

Implement Milestone 0 completely.

No shortcuts. Create the Rust workspace and crate skeletons exactly according to the architecture. Add examples, baseline tests, module docs, and enough CLI structure that future milestones have somewhere to attach commands.

Run:
- cargo fmt --all -- --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace

End with Summary, Validation, Definition of done, and Limitations.
```

## Then continue with

```text
Implement the next incomplete milestone from docs/ROADMAP.md completely. Follow AGENTS.md and docs/QUALITY_GATES.md. No shortcuts.
```

## Important workflow

Do not ask Codex for “the full browser” in one prompt. Ask it for one milestone at a time. That is how you get serious code instead of a fake demo.

After each milestone, run:

```text
Do a review-only pass using skills/review-quality/SKILL.md and docs/reviews/MILESTONE_REVIEW_TEMPLATE.md. Do not add features. Find issues that must be fixed before the next milestone.
```
