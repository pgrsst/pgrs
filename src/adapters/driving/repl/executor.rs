use nu_ansi_term::{Color, Style};
use unicode_width::UnicodeWidthChar;

use crate::core::ports::db_connection::QueryResult;

fn normalize_val(val: &str) -> &str {
    match val.to_lowercase().as_str() {
        "t" => "true",
        "f" => "false",
        _   => val,
    }
}

const MAX_CELL_CHARS: usize = 40;

fn truncate_middle(val: &str) -> String {
    let chars: Vec<char> = val.chars().collect();
    if chars.len() <= MAX_CELL_CHARS {
        return val.to_string();
    }
    let keep = MAX_CELL_CHARS - 3; // room for "..."
    let prefix = keep.div_ceil(2); // 19
    let suffix = keep - prefix;     // 18
    let head: String = chars[..prefix].iter().collect();
    let tail: String = chars[chars.len() - suffix..].iter().collect();
    format!("{}...{}", head, tail)
}

fn colorize_cell(val: &str) -> String {
    let display = normalize_val(val);
    if display.eq_ignore_ascii_case("true") {
        Style::new().fg(Color::Green).bold().paint(display).to_string()
    } else if display.eq_ignore_ascii_case("false") {
        Style::new().fg(Color::Red).bold().paint(display).to_string()
    } else if display.eq_ignore_ascii_case("null") {
        Style::new().dimmed().paint(display).to_string()
    } else {
        display.to_string()
    }
}

fn visible_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c.is_ascii_alphabetic() { in_escape = false; }
        } else {
            len += c.width().unwrap_or(0);
        }
    }
    len
}

pub fn format_result(result: &QueryResult, expanded: bool) -> String {
    if result.columns.is_empty() {
        let count = result.rows_affected.unwrap_or(result.rows.len() as u64);
        return if result.rows_affected.is_some() {
            format!("({} {})\n", count, if count == 1 { "row affected" } else { "rows affected" })
        } else {
            format!("({} {})\n", count, if count == 1 { "row" } else { "rows" })
        };
    }

    if expanded {
        return format_expanded(result);
    }

    format_minimal(result)
}

fn format_expanded(result: &QueryResult) -> String {
    let label_width = result.columns.iter().map(|c| c.len()).max().unwrap_or(0);
    let mut out = String::new();

    for (idx, row) in result.rows.iter().enumerate() {
        let title = format!("-[ RECORD {} ]", idx + 1);
        let pad = (label_width + 3).saturating_sub(visible_len(&title));
        out.push_str(&title);
        out.push_str(&"-".repeat(pad));
        out.push('\n');

        for (i, col) in result.columns.iter().enumerate() {
            let val = row.get(i).map(String::as_str).unwrap_or("NULL");
            let colored = colorize_cell(val);
            out.push_str(&format!("{:<width$} | {}\n", col, colored, width = label_width));
        }
    }

    let count = result.rows.len();
    out.push_str(&format!(
        "({} {})\n",
        count,
        if count == 1 { "row" } else { "rows" }
    ));

    out
}

fn format_minimal(result: &QueryResult) -> String {
    // pre-truncate each cell value (after t/f normalization)
    let cells: Vec<Vec<String>> = result
        .rows
        .iter()
        .map(|r| r.iter().map(|v| truncate_middle(normalize_val(v))).collect())
        .collect();

    let col_widths: Vec<usize> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let max_val = cells.iter().map(|r| r.get(i).map_or(0, |v| visible_len(v))).max().unwrap_or(0);
            col.len().max(max_val)
        })
        .collect();

    let mut out = String::new();

    // header
    let header: Vec<String> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| format!("{:<width$}", col, width = col_widths[i]))
        .collect();
    out.push_str(&header.join("  "));
    out.push('\n');

    // underline
    let underline: Vec<String> = col_widths.iter().map(|w| "─".repeat(*w)).collect();
    out.push_str(&underline.join("  "));
    out.push('\n');

    // rows
    for row in &cells {
        let line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, val)| {
                let colored = colorize_cell(val);
                let padding = col_widths[i].saturating_sub(visible_len(val));
                format!("{}{}", colored, " ".repeat(padding))
            })
            .collect();
        out.push_str(&line.join("  "));
        out.push('\n');
    }

    let count = result.rows.len();
    out.push_str(&format!(
        "({} {})\n",
        count,
        if count == 1 { "row" } else { "rows" }
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_single_row() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "email".to_string()],
            rows: vec![vec!["1".to_string(), "alice@example.com".to_string()]],
            rows_affected: None,
        };
        let out = format_result(&result, false);
        assert!(out.contains("id"), "missing column 'id'");
        assert!(out.contains("email"), "missing column 'email'");
        assert!(out.contains("1"), "missing value '1'");
        assert!(out.contains("alice@example.com"), "missing value");
        assert!(out.contains("(1 row)"), "missing row count");
    }

    #[test]
    fn formats_empty_result() {
        let result = QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
        };
        let out = format_result(&result, false);
        assert!(out.contains("(0 rows)"));
    }

    #[test]
    fn column_width_fits_longest_value() {
        let result = QueryResult {
            columns: vec!["name".to_string()],
            rows: vec![
                vec!["short".to_string()],
                vec!["a_very_long_name".to_string()],
            ],
            rows_affected: None,
        };
        let out = format_result(&result, false);
        assert!(out.contains("a_very_long_name"));
        assert!(out.contains("short"));
    }

    #[test]
    fn zero_row_select_shows_column_headers() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "email".to_string()],
            rows: vec![],
            rows_affected: Some(0),
        };
        let out = format_result(&result, false);
        assert!(out.contains("id"), "header 'id' missing");
        assert!(out.contains("email"), "header 'email' missing");
        assert!(out.contains("(0 rows)"), "row count missing");
    }

    #[test]
    fn dml_shows_rows_affected_label() {
        let result = QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: Some(3),
        };
        let out = format_result(&result, false);
        assert!(out.contains("(3 rows affected)"), "expected 'rows affected', got: {}", out);
    }

    #[test]
    fn dml_single_row_affected_singular() {
        let result = QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: Some(1),
        };
        let out = format_result(&result, false);
        assert!(out.contains("(1 row affected)"), "expected singular 'row affected', got: {}", out);
    }

    #[test]
    fn select_row_count_does_not_say_affected() {
        let result = QueryResult {
            columns: vec!["id".to_string()],
            rows: vec![vec!["1".to_string()]],
            rows_affected: None,
        };
        let out = format_result(&result, false);
        assert!(out.contains("(1 row)"), "SELECT should show '(1 row)', got: {}", out);
        assert!(!out.contains("affected"), "SELECT should not say 'affected', got: {}", out);
    }

    #[test]
    fn colorize_true_bold_green() {
        let result = colorize_cell("true");
        let expected = Style::new().fg(Color::Green).bold().paint("true").to_string();
        assert_eq!(result, expected);
    }

    #[test]
    fn colorize_false_bold_red() {
        let result = colorize_cell("false");
        let expected = Style::new().fg(Color::Red).bold().paint("false").to_string();
        assert_eq!(result, expected);
    }

    #[test]
    fn colorize_null_dim() {
        let result = colorize_cell("null");
        let expected = Style::new().dimmed().paint("null").to_string();
        assert_eq!(result, expected);
    }

    #[test]
    fn colorize_null_case_insensitive() {
        let result = colorize_cell("NULL");
        let expected = Style::new().dimmed().paint("NULL").to_string();
        assert_eq!(result, expected);
    }

    #[test]
    fn colorize_t_translates_to_true() {
        let result = colorize_cell("t");
        let expected = Style::new().fg(Color::Green).bold().paint("true").to_string();
        assert_eq!(result, expected, "'t' should be normalized to 'true' and colorized green");
    }

    #[test]
    fn colorize_f_translates_to_false() {
        let result = colorize_cell("f");
        let expected = Style::new().fg(Color::Red).bold().paint("false").to_string();
        assert_eq!(result, expected, "'f' should be normalized to 'false' and colorized red");
    }

    #[test]
    fn colorize_plain_value_unchanged() {
        let result = colorize_cell("hello");
        assert_eq!(result, "hello");
    }

    #[test]
    fn visible_len_strips_ansi() {
        let colored = colorize_cell("true");
        assert_eq!(visible_len(&colored), 4);
    }

    #[test]
    fn visible_len_plain_string() {
        assert_eq!(visible_len("hello"), 5);
    }

    #[test]
    fn visible_len_wide_cjk_chars() {
        // each CJK char is 2 display columns
        assert_eq!(visible_len("日本語"), 6);
    }

    #[test]
    fn visible_len_non_m_escape_terminator() {
        // cursor-movement escape \x1b[A must not corrupt subsequent chars
        assert_eq!(visible_len("\x1b[Ahello"), 5);
    }

    #[test]
    fn truncate_middle_keeps_short_values() {
        assert_eq!(truncate_middle("12345678910"), "12345678910");
    }

    #[test]
    fn truncate_middle_exactly_40_unchanged() {
        let s = "a".repeat(40);
        assert_eq!(truncate_middle(&s), s);
    }

    #[test]
    fn truncate_middle_long_value_has_ellipsis_total_40() {
        let s = "a".repeat(60);
        let out = truncate_middle(&s);
        assert_eq!(out.chars().count(), 40);
        assert!(out.contains("..."));
        assert!(out.starts_with(&"a".repeat(19)));
        assert!(out.ends_with(&"a".repeat(18)));
    }

    #[test]
    fn truncate_middle_char_based_multibyte() {
        // 50 CJK chars -> truncated to 40 chars, not bytes
        let s = "あ".repeat(50);
        let out = truncate_middle(&s);
        assert_eq!(out.chars().count(), 40);
        assert!(out.contains("..."));
    }

    #[test]
    fn minimal_uses_box_underline_and_two_space_gap() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "email".to_string()],
            rows: vec![vec!["1".to_string(), "alice@example.com".to_string()]],
            rows_affected: None,
        };
        let out = format_result(&result, false);
        assert!(out.contains('─'), "expected box-drawing underline, got:\n{out}");
        assert!(!out.contains('|'), "minimal style has no pipes, got:\n{out}");
        assert!(!out.contains('+'), "minimal style has no plus, got:\n{out}");
        assert!(out.contains("id  email"), "expected 2-space gap, got:\n{out}");
    }

    #[test]
    fn minimal_truncates_long_cell() {
        let long = "x".repeat(60);
        let result = QueryResult {
            columns: vec!["v".to_string()],
            rows: vec![vec![long.clone()]],
            rows_affected: None,
        };
        let out = format_result(&result, false);
        assert!(out.contains("..."), "expected truncated cell, got:\n{out}");
        assert!(!out.contains(&long), "full value should not appear, got:\n{out}");
    }

    #[test]
    fn expanded_uses_record_header_and_labels() {
        let result = QueryResult {
            columns: vec!["id".to_string(), "email".to_string()],
            rows: vec![
                vec!["1".to_string(), "alice@example.com".to_string()],
                vec!["2".to_string(), "bob@example.com".to_string()],
            ],
            rows_affected: None,
        };
        let out = format_result(&result, true);
        assert!(out.contains("-[ RECORD 1 ]"), "missing record 1 header:\n{out}");
        assert!(out.contains("-[ RECORD 2 ]"), "missing record 2 header:\n{out}");
        assert!(out.contains("email | alice@example.com"), "label padding wrong:\n{out}");
        assert!(out.contains("id    | 1"), "label padding wrong:\n{out}");
    }

    #[test]
    fn expanded_does_not_truncate() {
        let long = "y".repeat(60);
        let result = QueryResult {
            columns: vec!["v".to_string()],
            rows: vec![vec![long.clone()]],
            rows_affected: None,
        };
        let out = format_result(&result, true);
        assert!(out.contains(&long), "expanded mode must show full value:\n{out}");
        assert!(!out.contains("..."), "expanded mode must not truncate:\n{out}");
    }

    #[test]
    fn expanded_empty_columns_shows_footer_only() {
        let result = QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: Some(3),
        };
        let out = format_result(&result, true);
        assert!(out.contains("(3 rows affected)"), "expected footer:\n{out}");
        assert!(!out.contains("RECORD"), "no records expected:\n{out}");
    }
}
