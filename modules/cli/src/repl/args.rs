//! Shared, quote-aware tokenizer for backslash-command arguments.
//!
//! Several REPL commands take whitespace-separated arguments where one of them
//! may legitimately contain spaces (a saved-query name, a file path). `\export`
//! historically grew its own quote handling (`csv::parse_export_args`); this
//! module generalizes that so `\save`/`\run`/`\unsave` parse consistently and a
//! quoted name like `"my query"` survives as a single token.
//!
//! Kept deliberately simple: single or double quotes group a token, there are
//! no escape sequences, and an unterminated quote is taken literally to the end
//! of the line (lenient — never an error).

/// Split `rest` into tokens, honoring single- and double-quoted spans.
///
/// - Unquoted whitespace separates tokens; runs of whitespace collapse.
/// - A `'` or `"` at the start of a token opens a quoted span; everything up to
///   the matching closing quote (including spaces and the other quote char) is
///   taken literally. An unterminated quote consumes the rest of the line.
/// - A quote appearing mid-word (e.g. `ab"cd`) is kept literal — quotes only
///   delimit when they begin a token.
pub(super) fn tokenize_args(rest: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;

    for c in rest.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None; // closing quote ends the span but not the token
                } else {
                    current.push(c);
                }
            }
            None => {
                if c.is_whitespace() {
                    if in_token {
                        tokens.push(std::mem::take(&mut current));
                        in_token = false;
                    }
                } else if (c == '"' || c == '\'') && !in_token {
                    // Quote only opens a span when it begins a fresh token.
                    quote = Some(c);
                    in_token = true;
                } else {
                    current.push(c);
                    in_token = true;
                }
            }
        }
    }

    if in_token || quote.is_some() {
        tokens.push(current);
    }
    tokens
}

/// Resolve a command argument expected to be exactly one name token (possibly
/// quoted). Returns `None` when zero or more than one token is present, so the
/// caller can print its usage line.
pub(super) fn single_name_token(rest: &str) -> Option<String> {
    let mut toks = tokenize_args(rest);
    if toks.len() == 1 {
        Some(toks.remove(0))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_no_tokens() {
        assert!(tokenize_args("").is_empty());
        assert!(tokenize_args("   ").is_empty());
    }

    #[test]
    fn single_unquoted() {
        assert_eq!(tokenize_args("myq"), vec!["myq"]);
    }

    #[test]
    fn two_unquoted() {
        assert_eq!(tokenize_args("myq 42"), vec!["myq", "42"]);
    }

    #[test]
    fn extra_whitespace_collapsed() {
        assert_eq!(tokenize_args("  a   b  "), vec!["a", "b"]);
    }

    #[test]
    fn double_quoted_with_space() {
        assert_eq!(tokenize_args("\"my query\" 42"), vec!["my query", "42"]);
    }

    #[test]
    fn single_quoted_with_space() {
        assert_eq!(tokenize_args("'my query' 42"), vec!["my query", "42"]);
    }

    #[test]
    fn unterminated_quote_is_literal_remainder() {
        assert_eq!(tokenize_args("\"unclosed name"), vec!["unclosed name"]);
    }

    #[test]
    fn quote_inside_word_kept_literal() {
        assert_eq!(tokenize_args("ab\"cd"), vec!["ab\"cd"]);
    }

    #[test]
    fn mixed_quotes() {
        assert_eq!(tokenize_args("'a' \"b c\""), vec!["a", "b c"]);
    }

    #[test]
    fn single_name_token_accepts_one() {
        assert_eq!(single_name_token("myq"), Some("myq".to_string()));
        assert_eq!(single_name_token("\"my query\""), Some("my query".to_string()));
    }

    #[test]
    fn single_name_token_rejects_zero_or_many() {
        assert_eq!(single_name_token(""), None);
        assert_eq!(single_name_token("a b"), None);
    }
}
