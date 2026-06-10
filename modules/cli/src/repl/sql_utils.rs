//! REPL input helper: deciding when a typed line forms a complete statement so
//! the editor knows whether to keep buffering. This is a front-end concern (it
//! reflects how the REPL reads multi-line input), so it stays in the CLI. SQL
//! classification (DDL/DML) lives in `pgrs_core`.

/// Lexical state while scanning a partial statement. Anything other than
/// `Normal` at the end means the buffer is still "inside" something (a string,
/// an identifier, a comment, or a dollar-quoted body) and must keep buffering.
enum Lex {
    Normal,
    Single,            // '...'
    Double,            // "..."
    LineComment,       // -- ... up to newline
    BlockComment,      // /* ... */
    Dollar(String),    // $tag$ ... $tag$  (tag may be empty for $$)
}

/// If `chars[i]` opens a dollar-quote tag (`$$` or `$tag$`), return the tag and
/// the index just past the opening delimiter. PostgreSQL tags are identifiers
/// (letter/underscore start, then alphanumerics/underscores), so `$1` (a
/// positional parameter) and `$5.00` are correctly rejected as openers.
fn dollar_tag_at(chars: &[char], i: usize) -> Option<(String, usize)> {
    debug_assert_eq!(chars[i], '$');
    let mut j = i + 1;
    let mut tag = String::new();
    while j < chars.len() {
        let c = chars[j];
        if c == '$' {
            return Some((tag, j + 1));
        }
        let valid = if tag.is_empty() {
            c.is_ascii_alphabetic() || c == '_'
        } else {
            c.is_ascii_alphanumeric() || c == '_'
        };
        if !valid {
            return None;
        }
        tag.push(c);
        j += 1;
    }
    None
}

/// True when the buffered input forms a complete statement: a top-level `;`
/// terminates it and the scanner ends in `Normal` state (not mid-string,
/// mid-identifier, mid-comment, or mid-dollar-quote). Comments and
/// dollar-quoted bodies are skipped so semicolons/quotes inside them never
/// fool the terminator check.
pub(super) fn is_complete_statement(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let mut state = Lex::Normal;
    // The last non-whitespace character seen at top level (outside strings,
    // comments, and dollar quotes). The statement is complete iff this is `;`.
    let mut last_significant: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match state {
            Lex::Normal => {
                if c == '-' && next == Some('-') {
                    state = Lex::LineComment;
                    i += 2;
                    continue;
                }
                if c == '/' && next == Some('*') {
                    state = Lex::BlockComment;
                    i += 2;
                    continue;
                }
                if c == '$'
                    && let Some((tag, after)) = dollar_tag_at(&chars, i)
                {
                    state = Lex::Dollar(tag);
                    last_significant = Some('$');
                    i = after;
                    continue;
                }
                match c {
                    '\'' => {
                        state = Lex::Single;
                        last_significant = Some('\'');
                    }
                    '"' => {
                        state = Lex::Double;
                        last_significant = Some('"');
                    }
                    _ if !c.is_whitespace() => last_significant = Some(c),
                    _ => {}
                }
                i += 1;
            }
            Lex::Single => {
                if c == '\'' {
                    if next == Some('\'') {
                        i += 2; // escaped ''
                        continue;
                    }
                    state = Lex::Normal;
                }
                i += 1;
            }
            Lex::Double => {
                if c == '"' {
                    if next == Some('"') {
                        i += 2; // escaped ""
                        continue;
                    }
                    state = Lex::Normal;
                }
                i += 1;
            }
            Lex::LineComment => {
                if c == '\n' {
                    state = Lex::Normal;
                }
                i += 1;
            }
            Lex::BlockComment => {
                if c == '*' && next == Some('/') {
                    state = Lex::Normal;
                    i += 2;
                    continue;
                }
                i += 1;
            }
            Lex::Dollar(ref tag) => {
                if c == '$'
                    && let Some((close, after)) = dollar_tag_at(&chars, i)
                    && &close == tag
                {
                    state = Lex::Normal;
                    i = after;
                    continue;
                }
                i += 1;
            }
        }
    }
    // A trailing line comment is closed by end-of-input just as a newline would
    // close it, so it counts as a clean (Normal-equivalent) end state.
    matches!(state, Lex::Normal | Lex::LineComment) && last_significant == Some(';')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_unclosed_double_quoted_identifier_ending_with_semicolon_not_complete() {
        assert!(!is_complete_statement(r#"SELECT "col;"#));
    }

    #[test]
    fn complete_closed_double_quoted_identifier_then_semicolon_is_complete() {
        assert!(is_complete_statement(r#"SELECT "col;name" FROM t;"#));
    }

    #[test]
    fn complete_double_quoted_identifier_with_escaped_double_quote() {
        assert!(is_complete_statement(r#"SELECT "O""Brien" FROM t;"#));
    }

    #[test]
    fn complete_double_quote_inside_single_quote_does_not_open_identifier() {
        assert!(is_complete_statement(r#"SELECT '"quoted"' FROM t;"#));
    }

    #[test]
    fn complete_single_quote_inside_double_quote_does_not_open_string() {
        assert!(is_complete_statement(r#"SELECT "it's" FROM t;"#));
    }

    #[test]
    fn complete_no_semicolon_not_complete() {
        assert!(!is_complete_statement(r#"SELECT "col" FROM t"#));
    }

    #[test]
    fn complete_with_trailing_line_comment_after_semicolon() {
        assert!(is_complete_statement("SELECT * FROM users; -- done"));
    }

    #[test]
    fn complete_apostrophe_inside_line_comment_does_not_open_string() {
        assert!(is_complete_statement("SELECT id,\n name -- user's name\nFROM users;"));
    }

    #[test]
    fn incomplete_semicolon_inside_line_comment_is_not_terminator() {
        assert!(!is_complete_statement("SELECT 1 -- a;\n"));
    }

    #[test]
    fn complete_quote_inside_block_comment_does_not_open_string() {
        assert!(is_complete_statement("SELECT 1 /* it's fine */;"));
    }

    #[test]
    fn incomplete_block_comment_unterminated() {
        assert!(!is_complete_statement("SELECT 1 /* unterminated ;"));
    }

    #[test]
    fn complete_dollar_quoted_body_with_internal_semicolons() {
        assert!(is_complete_statement(
            "CREATE FUNCTION f() RETURNS int AS $$ BEGIN RETURN 1; END; $$ LANGUAGE plpgsql;"
        ));
    }

    #[test]
    fn incomplete_semicolon_inside_dollar_quote_is_not_terminator() {
        assert!(!is_complete_statement("DO $$ BEGIN PERFORM 1;"));
    }

    #[test]
    fn complete_tagged_dollar_quote() {
        assert!(is_complete_statement("SELECT $tag$ a; b $tag$;"));
    }

    #[test]
    fn complete_dollar_amount_is_not_dollar_quote() {
        // A bare `$` followed by a digit (e.g. positional param `$1` or text)
        // must not be mistaken for a dollar-quote opener.
        assert!(is_complete_statement("SELECT '$5.00'::text WHERE id = $1;"));
    }
}
