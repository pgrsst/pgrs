# generate-release gh Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the `generate-release` skill to include an interactive preview-and-revision loop followed by automated `gh release create`.

**Architecture:** Modify a single skill file (`.claude/skills/generate-release/SKILL.md`). Steps 1–4 remain unchanged. Add Step 5 (preview loop in chat) and expand Step 5 → Step 6 (git tag + `gh release create`).

**Tech Stack:** `gh` CLI (v2.46+), `git`, Markdown skill document.

---

### Task 1: Fix description in frontmatter

**Files:**
- Modify: `.claude/skills/generate-release/SKILL.md` (lines 1–8)

- [ ] **Step 1: Read the current frontmatter**

Open `.claude/skills/generate-release/SKILL.md` and note the current description block (lines 2–7).

- [ ] **Step 2: Replace the description**

The current description starts with a workflow summary, violating the "Use when..." rule. Replace the entire `description` block:

```yaml
---
name: generate-release
description: >
  Use when the user wants to create a release, generate a changelog,
  bump the version in Cargo.toml, or says "I want to release", "generate release
  notes", "what goes in the changelog", "bump version", or types /generate-release.
  Specific to Rust projects using Cargo.toml.
---
```

- [ ] **Step 3: Verify the frontmatter is valid YAML**

Run:
```bash
python3 -c "
import sys
content = open('.claude/skills/generate-release/SKILL.md').read()
block = content.split('---')[1]
import yaml; yaml.safe_load(block); print('OK')
"
```
Expected: `OK`

- [ ] **Step 4: Commit**

```bash
git add .claude/skills/generate-release/SKILL.md
git commit -m "fix(skill): description starts with Use when per CSO rules"
```

---

### Task 2: Add Step 5 — Preview & Revision Loop

**Files:**
- Modify: `.claude/skills/generate-release/SKILL.md`

- [ ] **Step 1: Locate the insertion point**

Find the line containing `### Step 5 — Git tag` in the file. The new Step 5 will be inserted BEFORE this line, and the old Step 5 will be renumbered to Step 6.

- [ ] **Step 2: Insert Step 5 block**

Add the following section immediately before `### Step 5 — Git tag`:

````markdown
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

- User replies **"ok" / "lanjut" / "publish" / "go"** → proceed to Step 6
- User requests a change → revise the release notes in the response → show the full preview again → repeat
- No iteration limit; continue until the user approves

````

- [ ] **Step 3: Verify the section was inserted correctly**

Run:
```bash
grep -n "Step 5\|Step 6" .claude/skills/generate-release/SKILL.md
```
Expected output (line numbers will vary):
```
NN:### Step 5 — Preview & revision loop
NN:### Step 5 — Git tag
```
(Step 6 will be renamed in the next task.)

- [ ] **Step 4: Commit**

```bash
git add .claude/skills/generate-release/SKILL.md
git commit -m "feat(skill): add Step 5 preview and revision loop"
```

---

### Task 3: Rename old Step 5 → Step 6 and expand with gh release create

**Files:**
- Modify: `.claude/skills/generate-release/SKILL.md`

- [ ] **Step 1: Rename the heading**

Change:
```markdown
### Step 5 — Git tag
```
To:
```markdown
### Step 6 — Publish release
```

- [ ] **Step 2: Replace the entire Step 6 body**

Remove the existing content under the old Step 5 (the two `git tag` / `git push` commands and their surrounding explanation) and replace with:

````markdown
Before publishing, verify `gh` is authenticated:

```bash
gh auth status
```

If not authenticated, stop and ask the user to run `gh auth login` before continuing.

Then create the tag and publish the release in one sequence:

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin vX.Y.Z
gh release create vX.Y.Z \
  --title "vX.Y.Z" \
  --notes "<approved release notes from Step 5>" \
  --target main
```

After the command succeeds, display the GitHub release URL that `gh` returns so the user can verify.
````

- [ ] **Step 3: Verify both steps are present and correctly numbered**

Run:
```bash
grep -n "^### Step" .claude/skills/generate-release/SKILL.md
```
Expected:
```
NN:### Step 1 — Determine the version
NN:### Step 2 — Read commit history
NN:### Step 3 — Bump version in Cargo.toml
NN:### Step 4 — Generate output
NN:### Step 5 — Preview & revision loop
NN:### Step 6 — Publish release
```

- [ ] **Step 4: Commit**

```bash
git add .claude/skills/generate-release/SKILL.md
git commit -m "feat(skill): add Step 6 gh release create with auth check"
```

---

### Task 4: Update Notes section

**Files:**
- Modify: `.claude/skills/generate-release/SKILL.md`

- [ ] **Step 1: Append gh-specific notes**

Find the `## Notes` section at the bottom of the file and append:

```markdown
- `gh` CLI must be installed and authenticated (`gh auth status`) before Step 6 runs
- Release notes passed to `gh release create --notes` are the approved text from Step 5 — never auto-generate from `--generate-notes` flag
- The `--target main` flag ensures the release points to the main branch; change if the default branch differs
```

- [ ] **Step 2: Verify the Notes section**

Run:
```bash
grep -A 10 "^## Notes" .claude/skills/generate-release/SKILL.md
```
Expected: all three new bullet points are present.

- [ ] **Step 3: Final sanity check — count all steps**

Run:
```bash
grep -c "^### Step" .claude/skills/generate-release/SKILL.md
```
Expected: `6`

- [ ] **Step 4: Commit**

```bash
git add .claude/skills/generate-release/SKILL.md
git commit -m "docs(skill): add gh CLI notes to generate-release"
```
