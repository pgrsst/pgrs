use crate::core::ports::db_connection::QueryResult;

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
            let max_val = result.rows.iter().map(|r| r[i].len()).max().unwrap_or(0);
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
            .map(|(i, val)| format!("{:<width$}", val, width = col_widths[i]))
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
}
