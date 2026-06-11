# `\edit` — Built-in Multiline SQL Editor for the REPL

Date: 2026-06-11
Status: Approved (brainstorming)

## Problem

Composing long multi-line SQL in the `shell` REPL is awkward. Input is buffered
by `SqlValidator` until a `;` terminates the statement, so the only way to keep
typing across lines is repeated Enter. Going back to edit an earlier line mid-
statement is painful, and the up/down arrows are bound to history rather than
in-buffer navigation. Users want a more comfortable way to write and edit a
multi-line statement before running it.

## Goal

Add a `\edit` REPL command that opens a built-in, in-process multiline editor
where Enter inserts a newline and an explicit key submits. On submit the SQL runs
through the existing execution path so all guards and side effects are identical
to typing SQL at the prompt.

## Decisions (from brainstorming)

- **Built-in, not `$EDITOR` shell-out.** Stay in-process using a second
  reedline instance configured for editing. No external editor.
- **Explicit command** `\edit` (alias `\e`, matching psql) — not an always-on
  mode and not a keybinding-only feature.
- **Empty buffer** on open. No seeding from history or saved queries (YAGNI).
- **Submit runs immediately.** Submit executes the SQL via `run_statement`;
  `Esc`/`Ctrl+C` cancels without executing.
- **No line numbers.** A line-number gutter is not feasible with reedline
  (`render_prompt_multiline_indicator()` returns the same string for every
  continuation line, with no line index), so the requirement was dropped in
  favour of keeping the existing SQL completion/highlighting. A live "line X"
  prompt indicator is likewise infeasible (the prompt does not re-render as the
  cursor moves between lines), so it is not attempted.

## Approach

Reuse reedline's own multiline mechanism rather than a TUI text widget:

- When a validator returns `ValidationResult::Incomplete`, reedline makes Enter
  insert a newline instead of submitting. An **always-Incomplete validator**
  therefore turns reedline into a free-form multiline editor.
- A keybinding bound to `ReedlineEvent::Submit` bypasses the validator to force
  submission. `Esc` bound to `ReedlineEvent::CtrlC` produces a cancel signal.

This keeps the existing SQL completion, highlighting, and hinting in the editor
for free, and routes execution through the single existing SQL path so the DML
transaction guard, analytics recording, DDL auto-refresh, timing, pager, and
transaction-state tracking all apply unchanged.

All changes live in `pgrs-cli`; `pgrs-core` is untouched.

## Components & Changes

### `repl/ui.rs`

- Add `build_editor_reedline(schema, table_freq, column_freq) -> Reedline`,
  mirroring `build_reedline` (same `SqlCompleter` / `SqlHighlighter` /
  `SqlHinter` and completion menu) but:
  - validator is a new `AlwaysIncomplete` (always returns
    `ValidationResult::Incomplete`), so Enter always inserts a newline;
  - keybindings add `Alt+Enter -> ReedlineEvent::Submit` and
    `Esc -> ReedlineEvent::CtrlC`; Tab keeps the existing completion binding.
- Add an `EditorPrompt` (or reuse `PgrsPrompt` with a distinct indicator) so the
  editor is visually distinct — indicator `edit> `, multiline indicator `   -> `.
- Add a `\edit` entry to `REPL_COMMANDS` (mentioning the `\e` alias) so it shows
  in `\help`.

### `repl/mod.rs`

- `ReplCommand`: add an `Edit` variant; `parse` maps `\edit` and `\e` to it.
- Dispatch handler for `Edit`:
  - build an editor reedline on demand from the current `schema.clone()` and the
    current `(table_freq, column_freq)` (so it always reflects the latest
    schema);
  - call `editor.read_line(&editor_prompt)`:
    - `Signal::Success(buf)` with non-empty trimmed text → `run_statement(buf, …)`
      (the same shared path used by plain SQL and `\run`);
    - empty buffer → no-op;
    - `Signal::CtrlC` / `Signal::CtrlD` → print `edit cancelled.` and return to
      the main prompt.

## Accepted Limitations

- SQL executed from the editor does **not** enter the main reedline up-arrow
  history (the editor is a separate instance), but it **is** recorded in
  analytics and therefore appears in `\history`. Accepted (YAGNI).
- No line-number gutter (see Decisions).

## Testing

- Parser unit tests: `\edit` and `\e` parse to `ReplCommand::Edit` (same pattern
  as existing `ReplCommand::parse` tests).
- `AlwaysIncomplete` validator always returns `ValidationResult::Incomplete`.
- Help text includes `\edit`.
- Interactive behaviour (newline-on-Enter, `Alt+Enter` submit, `Esc`/`Ctrl+C`
  cancel) is verified manually, consistent with the repo convention that
  interactive reedline components are not unit-tested.

## Out of Scope

- Seeding the editor from history / saved queries / running buffer.
- `$EDITOR` shell-out.
- Line numbers and a TUI text widget.
- Multi-statement splitting beyond what the current execution path already does.
