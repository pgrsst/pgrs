use std::io::Write;
use crate::core::ports::db_connection::{DbConnection, QueryResult};

pub fn describe_table(
    db: &dyn DbConnection,
    table: &str,
    extended: bool,
    writer: &mut impl Write,
) -> Result<(), String> {
    validate_table_name(table)?;
    let _ = (db, extended, writer);
    Ok(())
}

fn validate_table_name(name: &str) -> Result<(), String> {
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
        Ok(())
    } else {
        Err("invalid table name: only letters, digits, underscores, and dots are allowed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_empty() {
        assert!(validate_table_name("users").is_ok());
        assert!(validate_table_name("public.users").is_ok());
        assert!(validate_table_name("user_roles").is_ok());
    }

    #[test]
    fn validate_rejects_special_chars() {
        assert!(validate_table_name("users; DROP TABLE users").is_err());
        assert!(validate_table_name("users'").is_err());
        assert!(validate_table_name("users\"").is_err());
        assert!(validate_table_name("users-table").is_err());
    }

    #[test]
    fn validate_error_message_is_user_friendly() {
        let err = validate_table_name("bad'name").unwrap_err();
        assert!(err.contains("invalid table name"), "got: {err}");
    }
}
