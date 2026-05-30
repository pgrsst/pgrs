mod args;
mod common_handler;
mod connection_handler;

use pgrs_core::ConnectionApi;
use common_handler::CommonHandler;
use connection_handler::ConnectionHandler;

pub struct Cli {
    connection: ConnectionHandler,
    common: CommonHandler,
}

impl Cli {
    pub fn new(connection: ConnectionApi) -> Self {
        Self {
            connection: ConnectionHandler::new(connection),
            common: CommonHandler,
        }
    }

    pub fn run(&self, args: impl IntoIterator<Item = String>) -> Result<(), String> {
        let args: Vec<String> = args.into_iter().collect();

        match args.first().map(String::as_str) {
            None | Some("help") | Some("--help") | Some("-h") => self.common.help(),
            Some("add") => self.connection.add(&args[1..]),
            Some("list") | Some("ls") => self.connection.list(&args[1..]),
            Some("delete") | Some("del") | Some("rm") => self.connection.delete(&args[1..]),
            Some("edit") => self.connection.edit(&args[1..]),
            Some("rename") => self.connection.rename(&args[1..]),
            Some("connect") => self.connection.connect(&args[1..]),
            Some("completions") => self.common.completions(&args[1..]),
            Some("--version") | Some("-V") => self.common.version(),
            Some(cmd @ "shell") | Some(cmd @ "test") => Err(format!(
                "'{cmd}' requires a connection — run 'pgrs {cmd} <connection-name>'"
            )),
            Some(cmd) => Err(format!("unknown command '{cmd}'. Run 'pgrs' for help.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use pgrs_core::ConnectionApi;

    fn cli_with(names: &[&str]) -> Cli {
        Cli::new(ConnectionApi::in_memory_with(names))
    }

    #[test]
    fn no_args_returns_ok() {
        assert!(cli_with(&[]).run(std::iter::empty()).is_ok());
    }

    #[test]
    fn unknown_command_returns_error() {
        assert!(
            cli_with(&[])
                .run(["unknown".to_string()].into_iter())
                .is_err()
        );
    }

    #[test]
    fn unknown_command_error_mentions_command_name() {
        let err = cli_with(&[])
            .run(["foobar".to_string()].into_iter())
            .unwrap_err();
        assert!(
            err.contains("foobar"),
            "error should mention the unknown command, got: {err}"
        );
    }

    #[test]
    fn unknown_command_error_does_not_show_add_usage() {
        let err = cli_with(&[])
            .run(["foobar".to_string()].into_iter())
            .unwrap_err();
        assert!(
            !err.contains("--host"),
            "error should not show add usage, got: {err}"
        );
    }

    #[test]
    fn shell_command_via_cli_run_returns_error_with_usage() {
        let err = cli_with(&[])
            .run(["shell".to_string(), "prod".to_string()].into_iter())
            .unwrap_err();
        assert!(err.contains("shell"), "got: {err}");
    }

    #[test]
    fn shell_command_does_not_say_unknown_command() {
        let err = cli_with(&[])
            .run(["shell".to_string(), "prod".to_string()].into_iter())
            .unwrap_err();
        assert!(
            !err.contains("unknown command"),
            "should give specific error for shell, got: {err}"
        );
    }

    #[test]
    fn test_command_does_not_say_unknown_command() {
        let err = cli_with(&[])
            .run(["test".to_string(), "prod".to_string()].into_iter())
            .unwrap_err();
        assert!(
            !err.contains("unknown command"),
            "should give specific error for test, got: {err}"
        );
    }
}
