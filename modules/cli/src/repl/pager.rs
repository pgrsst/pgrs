use std::io::Write;

/// Decide whether `content` should be paged: paging must be enabled and the
/// content must have more lines than the terminal can show at once.
pub(super) fn should_page(content: &str, enabled: bool, term_rows: u16) -> bool {
    enabled && term_rows > 0 && content.lines().count() > term_rows as usize
}

/// Emit `content`: page it through `$PAGER` when appropriate, otherwise write it
/// to `writer`. Paging only happens for an interactive stdout; on any failure
/// (not a TTY, unknown size, spawn error) it falls back to a direct write so
/// output is never lost.
pub(super) fn emit(content: &str, enabled: bool, writer: &mut impl Write) {
    use std::io::IsTerminal;

    if enabled
        && std::io::stdout().is_terminal()
        && let Ok((_cols, rows)) = crossterm::terminal::size()
        && should_page(content, true, rows)
        && page(content).is_ok()
    {
        return;
    }
    write!(writer, "{}", content).ok();
}

/// Spawn `$PAGER` (fallback `less -SR`) and feed `content` to its stdin.
fn page(content: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};

    // $PAGER is split on whitespace into program + args. This does not handle
    // shell quoting (e.g. quoted args containing spaces); such values will
    // spawn-fail and fall back to a direct write via emit's error path.
    let pager = std::env::var("PAGER").unwrap_or_default();
    let mut cmd = if pager.trim().is_empty() {
        let mut c = Command::new("less");
        c.arg("-SR");
        c
    } else {
        let mut parts = pager.split_whitespace();
        // split_whitespace on a non-empty string yields at least one token.
        let mut c = Command::new(parts.next().unwrap());
        for arg in parts {
            c.arg(arg);
        }
        c
    };

    let mut child = cmd.stdin(Stdio::piped()).spawn()?;
    // Take (not borrow) the pipe so it is dropped at the end of this block,
    // closing it so the pager sees EOF before we wait for it to exit.
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_page_when_disabled() {
        let content = "a\nb\nc\nd\n";
        assert!(!should_page(content, false, 2));
    }

    #[test]
    fn pages_when_content_taller_than_terminal() {
        let content = "1\n2\n3\n4\n5\n";
        assert!(should_page(content, true, 3));
    }

    #[test]
    fn does_not_page_when_content_fits() {
        let content = "1\n2\n";
        assert!(!should_page(content, true, 24));
    }

    #[test]
    fn does_not_page_when_terminal_size_unknown() {
        let content = "1\n2\n3\n";
        assert!(!should_page(content, true, 0));
    }

    #[test]
    fn emit_writes_directly_when_not_a_tty() {
        // In the test harness stdout is not a TTY, so emit always writes to the
        // provided writer regardless of `enabled`.
        let mut buf = Vec::new();
        emit("hello\nworld\n", true, &mut buf);
        assert_eq!(String::from_utf8(buf).unwrap(), "hello\nworld\n");
    }
}
