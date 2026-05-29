use std::sync::Arc;

use crate::core::enums::tls_mode::TlsMode;
use crate::core::services::connection::service::{AddConnectionInput, ConnectionSvc, EditConnectionInput};
use super::args::{optional_option, parse_connection_url, parse_tls_mode};

pub struct ConnectionHandler {
    pub(super) svc: Arc<dyn ConnectionSvc>,
}

impl ConnectionHandler {
    pub fn new(svc: Arc<dyn ConnectionSvc>) -> Self {
        Self { svc }
    }

    pub fn add(&self, args: &[String]) -> Result<(), String> {
        let name = args.first().ok_or(
            "usage: pgrs add <name> [--url=<postgresql://...>] [--host=<host>] [--username=<user>] [--password=<pass>] [--database=<db>] [--port=<port>] [--tls=disable|require|verify-full]"
        )?.trim().to_string();

        let url = optional_option(args, "--url")
            .map(|u| parse_connection_url(&u))
            .transpose()?
            .unwrap_or_default();

        let host = optional_option(args, "--host")
            .or(url.host)
            .ok_or("host is required")?;
        let username = optional_option(args, "--username")
            .or(url.username)
            .ok_or("username is required")?;
        let password = optional_option(args, "--password")
            .or(url.password)
            .ok_or("password is required")?;
        let database = optional_option(args, "--database")
            .or(url.database)
            .ok_or("database is required")?;
        let port = optional_option(args, "--port")
            .map(|value| {
                value
                    .parse::<u16>()
                    .map_err(|_| "port must be a number".to_string())
            })
            .transpose()?
            .or(url.port)
            .unwrap_or(crate::core::domain::connection::DEFAULT_PORT);

        let tls = match optional_option(args, "--tls") {
            None => TlsMode::Disable,
            Some(s) => parse_tls_mode(&s)?,
        };

        self.svc.add_connection(AddConnectionInput {
            name: name.clone(),
            host,
            port,
            username,
            password,
            database,
            tls,
            environment: optional_option(args, "--env"),
        })?;

        println!("connection '{name}' added");
        Ok(())
    }

    pub fn list(&self, args: &[String]) -> Result<(), String> {
        let names_only = args.iter().any(|a| a == "--names-only");
        let connections = self.svc.list_connections()?;

        if names_only {
            for c in &connections {
                println!("{}", c.name);
            }
            return Ok(());
        }

        if connections.is_empty() {
            println!("no connections saved");
            return Ok(());
        }

        let name_w = connections.iter().map(|c| c.name.len()).max().unwrap_or(4).max(4);
        let host_w = connections.iter().map(|c| c.host.len()).max().unwrap_or(4).max(4);
        let db_w = connections.iter().map(|c| c.database.len()).max().unwrap_or(8).max(8);
        let user_w = connections.iter().map(|c| c.username.len()).max().unwrap_or(8).max(8);
        let tls_w = connections
            .iter()
            .map(|c| c.tls.to_string().len())
            .max()
            .unwrap_or(3)
            .max(3);
        let env_w = connections
            .iter()
            .map(|c| c.environment.as_deref().unwrap_or("").len())
            .max()
            .unwrap_or(3)
            .max(3);

        println!(
            "{:<8}  {:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<env_w$}  {:<user_w$}  {:<tls_w$}  PASSWORD",
            "ID", "NAME", "HOST", "PORT", "DATABASE", "ENV", "USERNAME", "TLS",
        );

        for c in &connections {
            println!(
                "{:<8}  {:<name_w$}  {:<host_w$}  {:<6}  {:<db_w$}  {:<env_w$}  {:<user_w$}  {:<tls_w$}  ****",
                c.id.map(|v| v.to_string()).as_deref().unwrap_or("-"),
                c.name, c.host, c.port, c.database,
                c.environment.as_deref().unwrap_or(""),
                c.username, c.tls,
            );
        }

        Ok(())
    }

    pub fn edit(&self, args: &[String]) -> Result<(), String> {
        let name = args
            .first()
            .ok_or("usage: pgrs edit <name> [--host=...] [--port=...] [--username=...] [--password=...] [--database=...] [--tls=...] [--env=...]")?
            .trim()
            .to_string();

        let port = optional_option(args, "--port")
            .map(|v| v.parse::<u16>().map_err(|_| "port must be a number".to_string()))
            .transpose()?;

        let tls = match optional_option(args, "--tls") {
            None => None,
            Some(s) => Some(parse_tls_mode(&s)?),
        };

        let environment = args.iter()
            .find(|a| a.starts_with("--env="))
            .map(|arg| {
                let val = arg.strip_prefix("--env=").unwrap();
                if val.is_empty() { None } else { Some(val.to_string()) }
            });

        let resolved_name = self.svc.find_connection(&name)?.name.clone();
        self.svc.edit_connection(&resolved_name, EditConnectionInput {
            host: optional_option(args, "--host"),
            port,
            username: optional_option(args, "--username"),
            password: optional_option(args, "--password"),
            database: optional_option(args, "--database"),
            tls,
            environment,
        })?;

        println!("connection '{resolved_name}' updated");
        Ok(())
    }

    pub fn rename(&self, args: &[String]) -> Result<(), String> {
        let old_name = args
            .first()
            .ok_or("usage: pgrs rename <old-name> <new-name>")?
            .trim()
            .to_string();
        let new_name = args
            .get(1)
            .ok_or("usage: pgrs rename <old-name> <new-name>")?
            .trim()
            .to_string();
        let resolved_old = self.svc.find_connection(&old_name)?.name.clone();
        self.svc.rename_connection(&resolved_old, &new_name)?;
        println!("connection '{resolved_old}' renamed to '{new_name}'");
        Ok(())
    }

    pub fn delete(&self, args: &[String]) -> Result<(), String> {
        let name = args
            .first()
            .ok_or("usage: pgrs delete <connection-name> [--yes]")?
            .trim()
            .to_string();

        let skip_confirmation = args.iter().any(|a| a == "--yes");

        if !skip_confirmation {
            use std::io::{self, IsTerminal, Write};
            if io::stdin().is_terminal() {
                print!("Delete connection '{name}'? [y/N] ");
                io::stdout().flush().map_err(|e| e.to_string())?;
                let mut input = String::new();
                io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
                if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                    println!("Aborted.");
                    return Ok(());
                }
            } else {
                return Err(format!(
                    "cowardly refusing to delete '{name}' — pass --yes to confirm"
                ));
            }
        }

        let resolved_name = self.svc.find_connection(&name)?.name.clone();
        self.svc.delete_connection(&resolved_name)?;
        println!("connection '{resolved_name}' deleted");
        Ok(())
    }

    pub fn connect(&self, args: &[String]) -> Result<(), String> {
        let name = args
            .first()
            .ok_or("usage: pgrs connect <connection-name>")?
            .trim()
            .to_string();

        let connection = self.svc.find_connection(&name)?;

        println!(
            "handing off to psql for '{}' — you'll return to your shell, not pgrs, on exit",
            connection.name
        );

        let mut cmd = std::process::Command::new("psql");
        cmd.env("PGPASSWORD", &connection.password)
            .arg("-h")
            .arg(&connection.host)
            .arg("-p")
            .arg(connection.port.to_string())
            .arg("-U")
            .arg(&connection.username)
            .arg("-d")
            .arg(&connection.database);

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let error = cmd.exec();
            Err(if error.kind() == std::io::ErrorKind::NotFound {
                "psql not found — is it installed?".to_string()
            } else {
                error.to_string()
            })
        }

        #[cfg(not(unix))]
        {
            match cmd.status() {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    Err("psql not found — is it installed?".to_string())
                }
                Err(e) => Err(e.to_string()),
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn get_connection(
        &self,
        name: &str,
    ) -> Result<crate::core::domain::connection::Connection, String> {
        self.svc.get_connection(name).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use super::ConnectionHandler;
    use crate::core::ports::connection_repository::test_support::StubConnectionRepository;
    use crate::core::services::connection::service::{ConnectionService, ConnectionSvc};

    fn handler_with(names: &[&str]) -> ConnectionHandler {
        ConnectionHandler::new(
            Arc::new(ConnectionService::new(Arc::new(StubConnectionRepository::with_names(names))))
                as Arc<dyn ConnectionSvc>,
        )
    }

    fn add_args(name: &str, extra: &[&str]) -> Vec<String> {
        let mut args = vec![
            name.to_string(),
            "--host=localhost".to_string(),
            "--username=user".to_string(),
            "--password=pass".to_string(),
            "--database=db".to_string(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        args
    }

    fn edit_args(name: &str, extra: &[&str]) -> Vec<String> {
        let mut args = vec![name.to_string()];
        args.extend(extra.iter().map(|s| s.to_string()));
        args
    }

    #[test]
    fn get_connection_returns_correct_connection() {
        let h = handler_with(&["prod"]);
        let conn = h.get_connection("prod").unwrap();
        assert_eq!(conn.name, "prod");
    }

    #[test]
    fn list_empty_returns_ok() {
        let h = handler_with(&[]);
        assert!(h.list(&[]).is_ok());
    }

    #[test]
    fn list_with_connections_returns_ok() {
        let h = handler_with(&["prod", "staging"]);
        assert!(h.list(&[]).is_ok());
    }

    #[test]
    fn list_names_only_returns_ok() {
        let h = handler_with(&["prod", "staging"]);
        assert!(h.list(&["--names-only".to_string()]).is_ok());
    }

    #[test]
    fn add_without_name_shows_add_usage() {
        let h = handler_with(&[]);
        let err = h.add(&[]).unwrap_err();
        assert!(err.contains("--host"), "error should show add usage, got: {err}");
        assert!(err.contains("add"), "error should mention add command, got: {err}");
    }

    #[test]
    fn add_without_tls_flag_defaults_to_disable() {
        use crate::core::enums::tls_mode::TlsMode;
        let h = handler_with(&[]);
        h.add(&add_args("prod", &[])).unwrap();
        assert_eq!(h.get_connection("prod").unwrap().tls, TlsMode::Disable);
    }

    #[test]
    fn add_with_tls_require_saves_require_mode() {
        use crate::core::enums::tls_mode::TlsMode;
        let h = handler_with(&[]);
        h.add(&add_args("prod", &["--tls=require"])).unwrap();
        assert_eq!(h.get_connection("prod").unwrap().tls, TlsMode::Require);
    }

    #[test]
    fn add_with_tls_verify_full_saves_verify_full_mode() {
        use crate::core::enums::tls_mode::TlsMode;
        let h = handler_with(&[]);
        h.add(&add_args("prod", &["--tls=verify-full"])).unwrap();
        assert_eq!(h.get_connection("prod").unwrap().tls, TlsMode::VerifyFull);
    }

    #[test]
    fn add_with_invalid_port_returns_error() {
        let h = handler_with(&[]);
        assert_eq!(h.add(&add_args("prod", &["--port=abc"])), Err("port must be a number".to_string()));
    }

    #[test]
    fn add_with_unknown_tls_returns_error() {
        let h = handler_with(&[]);
        let result = h.add(&add_args("prod", &["--tls=starttls"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown tls mode"));
    }

    #[test]
    fn add_with_url_sets_all_fields() {
        let h = handler_with(&[]);
        h.add(&[
            "prod".to_string(),
            "--url=postgresql://user:pass@localhost:5432/mydb".to_string(),
        ]).unwrap();
        let conn = h.get_connection("prod").unwrap();
        assert_eq!(conn.host, "localhost");
        assert_eq!(conn.port, 5432);
        assert_eq!(conn.username, "user");
        assert_eq!(conn.password, "pass");
        assert_eq!(conn.database, "mydb");
    }

    #[test]
    fn add_with_url_and_flag_overrides_port() {
        let h = handler_with(&[]);
        h.add(&[
            "prod".to_string(),
            "--url=postgresql://user:pass@localhost/mydb".to_string(),
            "--port=5433".to_string(),
        ]).unwrap();
        assert_eq!(h.get_connection("prod").unwrap().port, 5433);
    }

    #[test]
    fn add_with_url_postgres_scheme_accepted() {
        let h = handler_with(&[]);
        assert!(h.add(&[
            "prod".to_string(),
            "--url=postgres://user:pass@localhost/mydb".to_string(),
        ]).is_ok());
    }

    #[test]
    fn add_with_invalid_url_scheme_returns_error() {
        let h = handler_with(&[]);
        let err = h.add(&[
            "prod".to_string(),
            "--url=mysql://user:pass@host/db".to_string(),
        ]).unwrap_err();
        assert!(err.contains("postgresql://"), "got: {err}");
    }

    #[test]
    fn add_with_url_missing_password_requires_flag() {
        let h = handler_with(&[]);
        let result = h.add(&[
            "prod".to_string(),
            "--url=postgresql://user@localhost/mydb".to_string(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn add_with_env_flag_saves_environment() {
        let h = handler_with(&[]);
        h.add(&add_args("prod", &["--env=production"])).unwrap();
        assert_eq!(
            h.get_connection("prod").unwrap().environment.as_deref(),
            Some("production")
        );
    }

    #[test]
    fn add_without_env_flag_leaves_environment_none() {
        let h = handler_with(&[]);
        h.add(&add_args("prod", &[])).unwrap();
        assert_eq!(h.get_connection("prod").unwrap().environment, None);
    }

    #[test]
    fn edit_single_field_updates_database() {
        let h = handler_with(&["prod"]);
        h.edit(&edit_args("prod", &["--database=newdb"])).unwrap();
        assert_eq!(h.get_connection("prod").unwrap().database, "newdb");
    }

    #[test]
    fn edit_multiple_fields_updates_all() {
        let h = handler_with(&["prod"]);
        h.edit(&edit_args("prod", &["--password=secret2", "--database=newdb"])).unwrap();
        let conn = h.get_connection("prod").unwrap();
        assert_eq!(conn.password, "secret2");
        assert_eq!(conn.database, "newdb");
        assert_eq!(conn.host, "localhost"); // unchanged
    }

    #[test]
    fn edit_port_parses_correctly() {
        let h = handler_with(&["prod"]);
        h.edit(&edit_args("prod", &["--port=5433"])).unwrap();
        assert_eq!(h.get_connection("prod").unwrap().port, 5433);
    }

    #[test]
    fn edit_tls_updates_mode() {
        let h = handler_with(&["prod"]);
        h.edit(&edit_args("prod", &["--tls=require"])).unwrap();
        assert_eq!(
            h.get_connection("prod").unwrap().tls,
            crate::core::enums::tls_mode::TlsMode::Require
        );
    }

    #[test]
    fn edit_without_fields_returns_error() {
        let h = handler_with(&["prod"]);
        let err = h.edit(&edit_args("prod", &[])).unwrap_err();
        assert!(err.contains("at least one field"), "got: {err}");
    }

    #[test]
    fn edit_missing_name_returns_error() {
        let h = handler_with(&[]);
        let err = h.edit(&[]).unwrap_err();
        assert!(err.contains("edit"), "got: {err}");
    }

    #[test]
    fn edit_not_found_returns_error() {
        let h = handler_with(&[]);
        let err = h.edit(&edit_args("ghost", &["--password=x"])).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn edit_with_invalid_port_returns_error() {
        let h = handler_with(&["prod"]);
        assert_eq!(
            h.edit(&edit_args("prod", &["--port=abc"])),
            Err("port must be a number".to_string())
        );
    }

    #[test]
    fn edit_with_unknown_tls_returns_error() {
        let h = handler_with(&["prod"]);
        let result = h.edit(&edit_args("prod", &["--tls=starttls"]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown tls mode"));
    }

    #[test]
    fn edit_with_empty_host_returns_error() {
        let h = handler_with(&["prod"]);
        let result = h.edit(&edit_args("prod", &["--host="]));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("host is required"));
    }

    #[test]
    fn edit_env_flag_sets_environment() {
        let h = handler_with(&["prod"]);
        h.edit(&edit_args("prod", &["--env=staging"])).unwrap();
        assert_eq!(
            h.get_connection("prod").unwrap().environment.as_deref(),
            Some("staging")
        );
    }

    #[test]
    fn edit_empty_env_flag_clears_environment() {
        let h = handler_with(&["prod"]);
        h.edit(&edit_args("prod", &["--env=prod"])).unwrap();
        h.edit(&edit_args("prod", &["--env="])).unwrap();
        assert_eq!(h.get_connection("prod").unwrap().environment, None);
    }

    #[test]
    fn rename_succeeds() {
        let h = handler_with(&["prod"]);
        h.rename(&["prod".to_string(), "production".to_string()]).unwrap();
        assert!(h.get_connection("production").is_ok());
        assert!(h.get_connection("prod").is_err());
    }

    #[test]
    fn rename_missing_new_name_returns_error() {
        let h = handler_with(&["prod"]);
        let err = h.rename(&["prod".to_string()]).unwrap_err();
        assert!(err.contains("rename"), "got: {err}");
    }

    #[test]
    fn rename_missing_args_returns_error() {
        let h = handler_with(&[]);
        assert!(h.rename(&[]).is_err());
    }

    #[test]
    fn rename_not_found_returns_error() {
        let h = handler_with(&[]);
        let err = h.rename(&["ghost".to_string(), "new".to_string()]).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn delete_succeeds_returns_ok() {
        let h = handler_with(&["prod"]);
        assert!(h.delete(&["prod".to_string(), "--yes".to_string()]).is_ok());
    }

    #[test]
    fn delete_without_yes_in_non_tty_returns_error() {
        // test runner is not a TTY — --yes is required
        let h = handler_with(&["prod"]);
        let err = h.delete(&["prod".to_string()]).unwrap_err();
        assert!(err.contains("--yes"), "expected --yes hint, got: {err}");
    }

    #[test]
    fn delete_missing_name_arg_returns_error() {
        let h = handler_with(&[]);
        let result = h.delete(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("delete"));
    }

    #[test]
    fn connect_missing_name_arg_returns_error() {
        let h = handler_with(&[]);
        let result = h.connect(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("connect"));
    }

    #[test]
    fn connect_unknown_connection_returns_error() {
        let h = handler_with(&[]);
        let result = h.connect(&["nonexistent".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn connect_to_nonexistent_connection_returns_not_found() {
        let h = handler_with(&[]);
        let err = h.connect(&["ghost".to_string()]).unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }
}
