---
name: generate-release
description: >
  Use when the user wants to create a release, generate a changelog,
  bump the version in Cargo.toml, or says "I want to release", "generate release
  notes", "what goes in the changelog", "bump version", or types /generate-release.
  Specific to Rust projects using Cargo.toml.
---

# Generate Release

Automate release preparation: version bump, changelog generation, and release notes.

## Workflow

### Step 1 — Determine the version

If an argument is provided (`$ARGUMENTS`), use it as the target version (e.g. `1.2.0` or `minor`).
Otherwise, read the current version from `Cargo.toml` and determine the appropriate bump from commit history:

| Commit history contains | Bump |
|------------------------|------|
| `BREAKING CHANGE` or `feat!` | **major** |
| `feat:` | **minor** |
| Only `fix:`, `perf:`, `refactor:`, etc. | **patch** |

Follow **Semantic Versioning**: `MAJOR.MINOR.PATCH`

---

### Step 2 — Read commit history

```bash
git log <last-tag>..HEAD --oneline --no-merges
```

If no tag exists yet, use all commits: `git log --oneline --no-merges`

Group commits by type (feat, fix, perf, etc.).

---

### Step 3 — Bump version in Cargo.toml

Update the `version` field in `Cargo.toml` (root and workspace members if applicable).
Remember to update `Cargo.lock` by running `cargo check` or `cargo build` afterward — or remind the user to do so.

---

### Step 4 — Generate output

#### CHANGELOG entry (append to `CHANGELOG.md` if it exists)

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- ...

### Fixed
- ...

### Changed
- ...

### Performance
- ...
```

#### GitHub Release Notes (concise, for the release body)

```markdown
## What's Changed

### 🚀 New Features
- ...

### 🐛 Bug Fixes
- ...

### ⚡ Performance
- ...

**Full Changelog**: https://github.com/<owner>/<repo>/compare/vX.Y.Z-1...vX.Y.Z
```

---

### Step 5 — Preview & revision loop

Display a structured preview in chat — never ask the user to edit a file:

```
Version:  X.Y.Z-prev → X.Y.Z
Commits:
  - feat: ...
  - fix: ...

Release Notes:
---
## What's Changed
### 🚀 New Features
- ...

### 🐛 Bug Fixes
- ...

**Full Changelog**: https://github.com/<owner>/<repo>/compare/vX.Y.Z-prev...vX.Y.Z
---
Lanjutkan publish? (ok / minta revisi)
```

- User replies **"ok"** / **"lanjut"** / **"publish"** / **"go"** → approve and proceed to Step 6
- Any other reply → treat as a revision request: revise the release notes in the response → show the full preview again → repeat
- No iteration limit; continue until the user approves

---

### Step 5 — Git tag

Provide the commands to create the tag:

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin vX.Y.Z
```

---

## Notes

- For Rust workspaces (multiple crates), ask whether all crates should be bumped or only the ones that changed
- If `version.workspace = true` is used in `Cargo.toml`, only update the root workspace version
- For breaking changes, always highlight them at the top of the release notes
