#[cfg(test)]
mod comma_test {
    use super::*;
    
    #[test]
    fn test_comma_separated() {
        let m = build_alias_map("SELECT * FROM users u, orders o");
        assert_eq!(m.resolve("u"), Some("users"), "u should resolve to users");
        assert_eq!(m.resolve("o"), Some("orders"), "o should resolve to orders");
    }
}
