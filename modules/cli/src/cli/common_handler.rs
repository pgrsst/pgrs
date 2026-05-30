use crate::completions;

pub struct CommonHandler;

impl CommonHandler {
    pub fn help(&self) -> Result<(), String> {
        println!("{}", welcome());
        Ok(())
    }

    pub fn version(&self) -> Result<(), String> {
        println!("pgrs {}", env!("CARGO_PKG_VERSION"));
        Ok(())
    }

    pub fn completions(&self, args: &[String]) -> Result<(), String> {
        let shell = args
            .first()
            .ok_or("usage: pgrs completions <bash|zsh|fish>")?;
        let script = match shell.as_str() {
            "bash" => completions::bash_script(),
            "zsh" => completions::zsh_script(),
            "fish" => completions::fish_script(),
            other => {
                return Err(format!(
                    "unknown shell '{}' — supported: bash, zsh, fish",
                    other
                ));
            }
        };
        print!("{}", script);
        Ok(())
    }
}

struct CmdDoc {
    usage_lines: &'static [&'static str],
    desc_lines: &'static [&'static str],
}

// To add a new command: append one CmdDoc block here — no string wrangling required.
const COMMAND_DOCS: &[CmdDoc] = &[
    CmdDoc {
        usage_lines: &[
            "add <name> [--url=<postgresql://user:pass@host:port/db>]",
            "           [--host=<host>] [--username=<user>] [--password=<pass>]",
            "           [--database=<db>] [--port=<port>] [--tls=disable|require|verify-full]",
            "           [--env=<environment>]",
        ],
        desc_lines: &[
            "Add a new named connection. Use --url to specify all fields at once;",
            "individual flags override URL-parsed values.",
            "--tls: disable (no encryption), require (encrypt, no cert check),",
            "       verify-full (encrypt + verify server certificate)",
        ],
    },
    CmdDoc {
        usage_lines: &["list|ls [--names-only]"],
        desc_lines: &[
            "List all saved connections.",
            "--names-only: print only names, one per line (handy for scripts and shell completion)",
        ],
    },
    CmdDoc {
        usage_lines: &[
            "edit <name> [--host=...] [--port=...] [--username=...] [--password=...]",
            "            [--database=...] [--tls=...] [--env=...]",
        ],
        desc_lines: &["Update one or more fields of a saved connection"],
    },
    CmdDoc {
        usage_lines: &["delete|del|rm <name> [--yes]"],
        desc_lines: &["Delete a named connection (prompts for confirmation without --yes)"],
    },
    CmdDoc {
        usage_lines: &["rename <old-name> <new-name>"],
        desc_lines: &["Rename a saved connection"],
    },
    CmdDoc {
        usage_lines: &["test <name>"],
        desc_lines: &["Verify a saved connection is reachable"],
    },
    CmdDoc {
        usage_lines: &["connect <name>"],
        desc_lines: &["Open an interactive psql session using a saved connection"],
    },
    CmdDoc {
        usage_lines: &["shell <name>"],
        desc_lines: &["Open pgrs interactive SQL REPL with auto-completion"],
    },
    CmdDoc {
        usage_lines: &["completions <bash|zsh|fish>"],
        desc_lines: &["Print shell completion script"],
    },
    CmdDoc {
        usage_lines: &["help, --help, -h"],
        desc_lines: &["Show this help"],
    },
];

fn welcome() -> String {
    let mut out = String::from(
        "pgrs — PostgreSQL connection manager built with Rust\n\n\
         Manage and store named PostgreSQL connections locally.\n\n\
         Commands:\n",
    );
    for doc in COMMAND_DOCS {
        for line in doc.usage_lines {
            out.push_str(&format!("  {line}\n"));
        }
        for line in doc.desc_lines {
            out.push_str(&format!("             {line}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handler() -> CommonHandler {
        CommonHandler
    }

    #[test]
    fn help_command_returns_ok() {
        assert!(handler().help().is_ok());
    }

    #[test]
    fn version_flag_returns_ok() {
        assert!(handler().version().is_ok());
    }

    #[test]
    fn completions_bash_returns_ok() {
        assert!(handler().completions(&["bash".to_string()]).is_ok());
    }

    #[test]
    fn completions_zsh_returns_ok() {
        assert!(handler().completions(&["zsh".to_string()]).is_ok());
    }

    #[test]
    fn completions_fish_returns_ok() {
        assert!(handler().completions(&["fish".to_string()]).is_ok());
    }

    #[test]
    fn completions_unknown_shell_returns_err() {
        assert!(handler().completions(&["powershell".to_string()]).is_err());
    }

    #[test]
    fn completions_missing_arg_returns_err() {
        assert!(handler().completions(&[]).is_err());
    }

    #[test]
    fn welcome_lists_all_commands() {
        let w = welcome();
        for cmd in &["add", "list", "edit", "delete", "rename", "test", "connect", "shell", "completions"] {
            assert!(w.contains(cmd), "welcome should mention '{cmd}', got:\n{w}");
        }
    }
}
