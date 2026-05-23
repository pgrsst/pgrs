#[derive(Debug, Clone, PartialEq)]
pub enum SqlToken {
    Comment(String),
    StringLiteral(String),
    Number(String),
    Word(String),
    Other(char),
}

pub fn tokenize(input: &str) -> Vec<SqlToken> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < len {
        if chars[i] == '-' && i + 1 < len && chars[i + 1] == '-' {
            let start = i;
            while i < len && chars[i] != '\n' { i += 1; }
            tokens.push(SqlToken::Comment(chars[start..i].iter().collect()));
        } else if chars[i] == '\'' {
            let start = i;
            i += 1;
            loop {
                if i >= len { break; }
                if chars[i] == '\'' {
                    i += 1;
                    if i < len && chars[i] == '\'' { i += 1; } else { break; }
                } else { i += 1; }
            }
            tokens.push(SqlToken::StringLiteral(chars[start..i].iter().collect()));
        } else if chars[i].is_ascii_digit() {
            let start = i;
            let mut has_dot = false;
            while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !has_dot && i + 1 < len && chars[i + 1].is_ascii_digit())) {
                if chars[i] == '.' { has_dot = true; }
                i += 1;
            }
            tokens.push(SqlToken::Number(chars[start..i].iter().collect()));
        } else if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
            tokens.push(SqlToken::Word(chars[start..i].iter().collect()));
        } else {
            tokens.push(SqlToken::Other(chars[i]));
            i += 1;
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_keyword_becomes_word() {
        let tokens = tokenize("SELECT");
        assert_eq!(tokens, vec![SqlToken::Word("SELECT".to_string())]);
    }

    #[test]
    fn tokenize_string_literal() {
        let tokens = tokenize("'hello'");
        assert_eq!(tokens, vec![SqlToken::StringLiteral("'hello'".to_string())]);
    }

    #[test]
    fn tokenize_number_integer() {
        let tokens = tokenize("42");
        assert_eq!(tokens, vec![SqlToken::Number("42".to_string())]);
    }

    #[test]
    fn tokenize_number_decimal() {
        let tokens = tokenize("3.14");
        assert_eq!(tokens, vec![SqlToken::Number("3.14".to_string())]);
    }

    #[test]
    fn tokenize_comment_to_eol() {
        let tokens = tokenize("-- note");
        assert_eq!(tokens, vec![SqlToken::Comment("-- note".to_string())]);
    }

    #[test]
    fn tokenize_escaped_single_quote_in_string() {
        let tokens = tokenize("'O''Brien'");
        assert_eq!(tokens, vec![SqlToken::StringLiteral("'O''Brien'".to_string())]);
    }

    #[test]
    fn tokenize_number_trailing_dot_not_consumed() {
        let tokens = tokenize("10.");
        assert_eq!(tokens[0], SqlToken::Number("10".to_string()));
        assert_eq!(tokens[1], SqlToken::Other('.'));
    }
}
