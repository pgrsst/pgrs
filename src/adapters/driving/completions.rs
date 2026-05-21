pub fn bash_script() -> &'static str {
    include_str!("completions/pgrs.bash")
}

pub fn zsh_script() -> &'static str {
    include_str!("completions/pgrs.zsh")
}

pub fn fish_script() -> &'static str {
    include_str!("completions/pgrs.fish")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_script_contains_subcommands() {
        let s = bash_script();
        assert!(s.contains("add"), "bash script missing 'add'");
        assert!(s.contains("list"), "bash script missing 'list'");
        assert!(s.contains("delete"), "bash script missing 'delete'");
        assert!(s.contains("connect"), "bash script missing 'connect'");
        assert!(s.contains("shell"), "bash script missing 'shell'");
        assert!(s.contains("completions"), "bash script missing 'completions'");
        assert!(s.contains("--names-only"), "bash script must call --names-only for dynamic names");
    }

    #[test]
    fn zsh_script_contains_subcommands() {
        let s = zsh_script();
        assert!(s.contains("add"));
        assert!(s.contains("shell"));
        assert!(s.contains("--names-only"));
    }

    #[test]
    fn fish_script_contains_subcommands() {
        let s = fish_script();
        assert!(s.contains("add"));
        assert!(s.contains("shell"));
        assert!(s.contains("--names-only"));
    }
}
