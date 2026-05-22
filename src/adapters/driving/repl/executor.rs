use crate::core::ports::db_connection::QueryResult;

fn colorize_cell(val: &str) -> String {
    match val.to_lowercase().as_str() {
        "true"  => format!("\x1b[1;32m{}\x1b[0m", val),
        "false" => format!("\x1b[1;31m{}\x1b[0m", val),
        "null"  => format!("\x1b[2m{}\x1b[0m", val),
        _       => val.to_string(),
    }
}

fn visible_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c == 'm' { in_escape = false; }
        } else {
            len += 1;
        }
    }
    len
}

pub fn print_result(result: &QueryResult) {
    print!("{}", format_result(result));
}

pub fn format_result(result: &QueryResult) -> String {
    if result.columns.is_empty() {
        let count = result.rows.len();
        return format!("({} {})\n", count, if count == 1 { "row" } else { "rows" });
    }

    let col_widths: Vec<usize> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let max_val = result.rows.iter().map(|r| visible_len(&r[i])).max().unwrap_or(0);
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
    out.push_str(&format!(" {} \n", header.join(" | ")));

    // separator
    let sep: Vec<String> = col_widths.iter().map(|w| "-".repeat(*w + 2)).collect();
    out.push_str(&sep.join("+"));
    out.push('\n');

    // rows
    for row in &result.rows {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, val)| {
                let colored = colorize_cell(val);
                let padding = col_widths[i].saturating_sub(visible_len(val));
                format!("{}{}", colored, " ".repeat(padding))
            })
            .collect();
        out.push_str(&format!(" {} \n", cells.join(" | ")));
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
        };
        let out = format_result(&result);
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
        };
        let out = format_result(&result);
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
        };
        let out = format_result(&result);
        assert!(out.contains("a_very_long_name"));
        assert!(out.contains("short"));
    }

    #[test]
    fn colorize_true_bold_green() {
        let result = colorize_cell("true");
        assert!(result.contains("\x1b[1;32m"), "expected bold green for true");
        assert!(result.contains("true"));
        assert!(result.contains("\x1b[0m"));
    }

    #[test]
    fn colorize_false_bold_red() {
        let result = colorize_cell("false");
        assert!(result.contains("\x1b[1;31m"), "expected bold red for false");
        assert!(result.contains("false"));
    }

    #[test]
    fn colorize_null_dim() {
        let result = colorize_cell("null");
        assert!(result.contains("\x1b[2m"), "expected dim for null");
        assert!(result.contains("null"));
    }

    #[test]
    fn colorize_null_case_insensitive() {
        let result = colorize_cell("NULL");
        assert!(result.contains("\x1b[2m"), "expected dim for NULL");
    }

    #[test]
    fn colorize_plain_value_unchanged() {
        let result = colorize_cell("hello");
        assert_eq!(result, "hello");
    }

    #[test]
    fn visible_len_strips_ansi() {
        let colored = "\x1b[1;32mtrue\x1b[0m";
        assert_eq!(visible_len(colored), 4);
    }

    #[test]
    fn visible_len_plain_string() {
        assert_eq!(visible_len("hello"), 5);
    }
}
