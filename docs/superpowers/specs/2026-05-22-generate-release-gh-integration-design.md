# Design: generate-release gh Integration

**Date:** 2026-05-22  
**Scope:** Extend `generate-release` skill to publish GitHub releases via `gh` CLI

## Background

The existing `generate-release` skill handles version bumping, CHANGELOG generation, and release notes drafting, ending at Step 5 with manually-provided `git tag` commands. This design extends it with an interactive preview-and-revision loop followed by automated `gh release create`.

## Requirements

- User reviews release notes in chat before anything is published
- User can request revisions via prompt (no manual file editing)
- After approval, the release is published to GitHub automatically
- Binary assets are NOT uploaded (release notes only)
- `gh` CLI is assumed to be installed and authenticated

## Workflow

Steps 1–4 remain unchanged from the current skill:

1. Determine version bump
2. Read commit history (`git log <last-tag>..HEAD`)
3. Bump version in `Cargo.toml` + run `cargo check`
4. Generate CHANGELOG entry + draft release notes

### Step 5 — Preview & Revision Loop (new)

Claude displays a structured preview in chat:

```
Version:  0.2.0 → 0.3.0
Commits:
  - feat: add delete command (#12)
  - fix: handle missing config file (#11)

Release Notes:
---
## What's Changed
### 🚀 New Features
- Add delete command

### 🐛 Bug Fixes
- Handle missing config file

**Full Changelog**: https://github.com/pgrsst/pgrs/compare/v0.2.0...v0.3.0
---
Lanjutkan publish? (ok / minta revisi)
```

- User responds "ok" / "lanjut" / "publish" / "go" → proceed to Step 6
- User requests a change → Claude revises notes → shows full preview again → loop
- No iteration limit; user decides when ready

### Step 6 — Publish Release (new)

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin vX.Y.Z
gh release create vX.Y.Z \
  --title "vX.Y.Z" \
  --notes "<approved release notes>" \
  --target main
```

Claude confirms success by showing the GitHub release URL returned by `gh`.

## Out of Scope

- Binary/asset uploads
- Draft releases
- Manual file editing by user
- Multi-crate workspace version bumping (existing Notes section covers this)

## Constraints

- `gh auth status` must pass before Step 6 runs; if not, surface a clear error
- CHANGELOG.md is updated in Step 4 (before preview), not after
