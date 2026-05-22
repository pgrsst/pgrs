---
name: generate-commit
description: >
  Use when the user wants to commit, needs a commit message, asks
  "what should the commit message be", "generate a commit message",
  or types /generate-commit. Also triggers when user says
  "I want to commit" or "generate commit".
---

# Generate Commit Message

## Overview

Generate commit messages following **Conventional Commits** format.
This skill only **generates** the message — it does not run `git commit` unless the user explicitly asks.

## Format

```
<type>(<scope>): <short description>

[optional body]

[optional footer]
```

### Types

| Type | When to use |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `refactor` | Refactor without behavior change |
| `perf` | Performance improvement |
| `test` | Add or fix tests |
| `docs` | Documentation |
| `chore` | Build, dependencies, config |
| `style` | Formatting, no logic change |

### Scope (optional)

Use the name of the module/crate being changed. Examples: `(cli)`, `(service)`, `(repository)`, `(domain)`.

Optional argument: `$ARGUMENTS` — treated as a scope hint. Example: `/generate-commit auth` → use `(auth)` as the default scope.

---

## Workflow

1. Run `git diff --staged` to inspect staged changes
2. If no staged changes, run `git diff` to see unstaged changes
3. If both are empty, inform the user there are no changes to commit and stop
4. Analyze the changes: what changed, why (inferred from code context), and the impact
5. Generate the commit message

### Rules

- Subject line **max 72 characters**
- Subject line uses **imperative mood** ("add" not "added", "fix" not "fixed")
- No period at the end of the subject line
- Body (if needed): explain *why*, not *what* — the *what* is already visible in the diff
- Breaking change: add `BREAKING CHANGE:` in the footer

---

## Output Format

Present in a code block for easy copying:

```
<generated commit message>
```

If there are multiple logically unrelated changes, offer the option to split them into separate commits.

### Example

```
feat(cli): add --port flag to connection config

Port defaults to 5432 when omitted; flag accepts values 1–65535.
```

---

## Common Mistakes

- **Using past tense** ("added", "fixed") — always use imperative mood ("add", "fix")
- **Subject line too long** — keep under 72 characters
- **Body explains what, not why** — the diff already shows what changed; body should explain the reasoning
- **Running `git commit` without being asked** — only generate the message unless the user explicitly requests to commit
