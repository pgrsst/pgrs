# `\edit` Built-in Multiline SQL Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `\edit` (alias `\e`) REPL command that opens a built-in multiline SQL editor where Enter inserts a newline and Alt+Enter submits & executes, while Esc/Ctrl+C cancels.

**Architecture:** Reuse reedline as the editor by giving a second `Reedline` instance an always-`Incomplete` validator (so Enter inserts newlines) plus an `Alt+Enter → Submit` keybinding. Execution routes through the existing `run_statement` path so all guards/side-effects are identical to typing SQL. All changes are in `pgrs-cli`; `pgrs-core` is untouched.

**Tech Stack:** Rust, reedline (REPL/editor), existing `SqlCompleter`/`SqlHighlighter`/`SqlHinter`.

Spec: `docs/superpowers/specs/2026-06-11-repl-edit-command-design.md`

---

### Task 1: Parse `\edit` / `\e` into a new `ReplCommand::Edit`

**Files:**
- Modify: `modules/cli/src/repl/mod.rs` (enum `ReplCommand` ~line 153, `parse` ~line 179, tests ~line 426)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `modules/cli/src/repl/mod.rs` (alongside the other `ReplCommand::parse` tests):

```rust
    #[test]
    fn edit_command_and_alias_parse() {
        assert!(matches!(ReplCommand::parse("\\edit"), ReplCommand::Edit));
        assert!(matches!(ReplCommand::parse("\\e"), ReplCommand::Edit));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pgrs-cli edit_command_and_alias_parse`
Expected: FAIL — `no variant named Edit found for enum ReplCommand` (compile error).

- [ ] **Step 3: Add the enum variant and parse arms**

In `enum ReplCommand<'a>` add the variant (near the other no-arg variants, e.g. after `Refresh,`):

```rust
    Edit,            // \edit / \e
```

In `ReplCommand::parse`, add two arms to the exact-match `match trimmed { … }` block (next to `"\\refresh" => ReplCommand::Refresh,`):

```rust
            "\\edit" | "\\e" => ReplCommand::Edit,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pgrs-cli edit_command_and_alias_parse`
Expected: PASS.

Note: the dispatch `match` in `Repl::run` is now non-exhaustive and the crate will not fully compile until Task 3. That is expected; this task's unit test still compiles and passes because the test module only references `ReplCommand::parse`. If `cargo test -p pgrs-cli` fails to build due to the missing arm, run just this test with `cargo test -p pgrs-cli --lib edit_command_and_alias_parse` after Task 3, or proceed — Task 3 completes the build. To keep the build green between tasks, add a temporary `ReplCommand::Edit => {}` arm in the dispatch `match` now and replace it in Task 3.

- [ ] **Step 5: Commit**

```bash
git add modules/cli/src/repl/mod.rs
git commit -m "feat(repl): parse \\edit and \\e into ReplCommand::Edit"
```

---

### Task 2: Editor reedline builder + `AlwaysIncomplete` validator + `EditorPrompt`

**Files:**
- Modify: `modules/cli/src/repl/ui.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `modules/cli/src/repl/ui.rs`:

```rust
    #[test]
    fn always_incomplete_validator_never_completes() {
        let v = AlwaysIncomplete;
        assert!(matches!(v.validate("SELECT 1;"), ValidationResult::Incomplete));
        assert!(matches!(v.validate(""), ValidationResult::Incomplete));
    }

    #[test]
    fn editor_prompt_indicator_is_edit() {
        let p = EditorPrompt;
        assert_eq!(
            p.render_prompt_indicator(reedline::PromptEditMode::Default).as_ref(),
            "edit> "
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pgrs-cli always_incomplete_validator_never_completes`
Expected: FAIL — `cannot find type AlwaysIncomplete` / `cannot find value EditorPrompt` (compile error).

- [ ] **Step 3: Implement validator, prompt, and builder**

In `modules/cli/src/repl/ui.rs`, after the existing `SqlValidator` impl (~line 69), add:

```rust
/// Validator for the `\edit` multiline editor: always Incomplete so Enter
/// inserts a newline instead of submitting. Submission is driven by an explicit
/// `Alt+Enter -> Submit` keybinding (see `build_editor_reedline`).
struct AlwaysIncomplete;

impl Validator for AlwaysIncomplete {
    fn validate(&self, _line: &str) -> ValidationResult {
        ValidationResult::Incomplete
    }
}

/// Prompt for the `\edit` editor — visually distinct from the main prompt so the
/// user knows Enter inserts a newline and Alt+Enter submits.
pub(super) struct EditorPrompt;

impl Prompt for EditorPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("edit> ")
    }
    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed("   -> ")
    }
    fn render_prompt_history_search_indicator(
        &self,
        _history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Borrowed("")
    }
}
```

Then add the builder after `build_reedline` (~line 147). It mirrors `build_reedline` but swaps the validator and adds the submit/cancel keybindings:

```rust
/// Build a reedline configured as a multiline SQL editor for `\edit`: Enter
/// inserts a newline (always-Incomplete validator), `Alt+Enter` submits, `Esc`
/// cancels. Reuses the same completion/highlighting/hinting as the main prompt.
pub(super) fn build_editor_reedline(
    schema: SchemaApi,
    table_freq: HashMap<String, u64>,
    column_freq: HashMap<String, u64>,
) -> Reedline {
    let highlighter = SqlHighlighter::new(schema.clone());
    let hinter = SqlHinter::new(schema.clone(), table_freq.clone(), column_freq.clone());
    let completer = SqlCompleter::new(schema, table_freq, column_freq);

    let menu = ColumnarMenu::default().with_name("completion_menu");

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::HistoryHintComplete,
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );
    // Alt+Enter submits the whole buffer (bypasses the always-Incomplete validator).
    keybindings.add_binding(KeyModifiers::ALT, KeyCode::Enter, ReedlineEvent::Submit);
    // Esc cancels the edit (same outcome as Ctrl+C: a CtrlC signal).
    keybindings.add_binding(KeyModifiers::NONE, KeyCode::Esc, ReedlineEvent::CtrlC);

    Reedline::create()
        .with_completer(Box::new(completer))
        .with_hinter(Box::new(hinter))
        .with_highlighter(Box::new(highlighter))
        .with_validator(Box::new(AlwaysIncomplete))
        .with_menu(ReedlineMenu::EngineCompleter(Box::new(menu)))
        .with_quick_completions(true)
        .with_partial_completions(true)
        .with_edit_mode(Box::new(Emacs::new(keybindings)))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pgrs-cli always_incomplete_validator_never_completes editor_prompt_indicator_is_edit`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
git add modules/cli/src/repl/ui.rs
git commit -m "feat(repl): editor reedline builder, AlwaysIncomplete validator, EditorPrompt"
```

---

### Task 3: Dispatch `\edit` — open the editor and run the result

**Files:**
- Modify: `modules/cli/src/repl/mod.rs` (the `match ReplCommand::parse(trimmed)` block in `Repl::run`, ~line 301)

- [ ] **Step 1: Add the dispatch arm**

In the dispatch `match` inside `Repl::run`, add an arm for `ReplCommand::Edit` (replace the temporary `ReplCommand::Edit => {}` arm if you added one in Task 1). Place it near `ReplCommand::Refresh`:

```rust
                        ReplCommand::Edit => {
                            let (tf, cf) =
                                freq_for_schema(&analytics, &connection_name, &schema);
                            let mut editor =
                                ui::build_editor_reedline(schema.clone(), tf, cf);
                            match editor.read_line(&ui::EditorPrompt) {
                                Ok(Signal::Success(buf)) => {
                                    if !buf.trim().is_empty() {
                                        run_statement(
                                            &handler, &query, &buf, expanded, timing,
                                            pager_enabled, &connection_name, &analytics,
                                            &mut schema, &mut rl, &tx, &mut stdout,
                                        );
                                    }
                                }
                                Ok(Signal::CtrlC) | Ok(Signal::CtrlD) => {
                                    writeln!(stdout, "edit cancelled.").ok();
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    writeln!(stdout, "error: {e}").ok();
                                }
                            }
                        }
```

- [ ] **Step 2: Build to verify it compiles**

Run: `cargo build -p pgrs-cli`
Expected: builds cleanly (the dispatch `match` is now exhaustive).

- [ ] **Step 3: Run the full crate test suite**

Run: `cargo test -p pgrs-cli`
Expected: PASS, including `edit_command_and_alias_parse` from Task 1.

- [ ] **Step 4: Commit**

```bash
git add modules/cli/src/repl/mod.rs
git commit -m "feat(repl): dispatch \\edit to the multiline editor and run on submit"
```

---

### Task 4: Surface `\edit` in REPL help

**Files:**
- Modify: `modules/cli/src/repl/ui.rs` (`REPL_COMMANDS` ~line 72, tests ~line 149)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `modules/cli/src/repl/ui.rs`:

```rust
    #[test]
    fn help_text_mentions_edit_command() {
        let text = repl_help_text();
        assert!(text.contains("\\edit"), "help should mention \\edit, got: {text}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pgrs-cli help_text_mentions_edit_command`
Expected: FAIL — assertion fails (help does not yet mention `\edit`).

- [ ] **Step 3: Add the help entry**

In `REPL_COMMANDS` (in `modules/cli/src/repl/ui.rs`), add an entry (place it near `\refresh`):

```rust
    ("\\edit, \\e",          "open a multiline editor (Alt+Enter runs, Esc cancels)"),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p pgrs-cli help_text_mentions_edit_command`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add modules/cli/src/repl/ui.rs
git commit -m "docs(repl): list \\edit in help text"
```

---

### Task 5: Workspace verification + manual smoke test

**Files:** none (verification only)

- [ ] **Step 1: Full workspace check**

Run: `cargo clippy --workspace && cargo test --workspace`
Expected: no clippy warnings introduced; all tests pass.

- [ ] **Step 2: Manual interactive smoke test**

Run (against any configured connection `<name>`): `cargo run -- shell <name>`

Verify by hand (interactive reedline behaviour is not unit-tested, per repo convention):
- `\edit` → prompt changes to `edit> `.
- Enter inserts a newline (continuation lines show `   -> `); the buffer does not submit.
- Type a multi-line `SELECT`, then `Alt+Enter` → the query runs and results print (timing/pager/`\x` behave as for normal SQL).
- `\edit` again, type something, press `Esc` → prints `edit cancelled.`, nothing runs.
- `\edit`, press `Ctrl+C` → prints `edit cancelled.`.
- With no open transaction, `\edit` an `INSERT …` then `Alt+Enter` → rejected by the DML guard (same message as typing the INSERT directly).
- `\help` lists `\edit, \e`.

- [ ] **Step 3: Final commit (if any cleanup was needed)**

```bash
git add -A
git commit -m "chore(repl): finalize \\edit editor" --allow-empty
```

---

## Self-Review Notes

- **Spec coverage:** `\edit`+`\e` command (Task 1, 3), empty buffer (Task 3 — editor opens fresh, no seeding), Alt+Enter submit→execute via `run_statement` (Task 3), Esc/Ctrl+C cancel (Task 2 keybinding + Task 3 handling), reuse completion/highlight/hint (Task 2 builder), `EditorPrompt` distinct visual (Task 2), no line numbers (not implemented — by design), help entry (Task 4), DML guard/analytics/auto-refresh/timing/pager inherited via `run_statement` (Task 3). All spec requirements mapped.
- **Accepted limitation** (editor SQL not in main reedline up-arrow history) needs no code — it follows from using a separate `Reedline` instance; analytics `\history` still records it via `run_statement`.
- **Type consistency:** `build_editor_reedline`, `AlwaysIncomplete`, `EditorPrompt`, `ReplCommand::Edit` used identically across tasks; `run_statement` signature matches the existing call site at `mod.rs:404`.
