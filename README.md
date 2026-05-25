# pgrs

A CLI tool for managing named PostgreSQL connection configurations, built with Rust.

## Requirements

- Linux or macOS
- `psql` (PostgreSQL client) — required for `pgrs connect`

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/pgrsst/pgrs/main/install.sh | bash
```

After installing, restart your terminal or run:

```bash
source ~/.bashrc  # or ~/.zshrc if using zsh
```

> **Security note:** The script downloads a binary directly from [GitHub Releases](https://github.com/pgrsst/pgrs/releases). Make sure you trust the release content before running the command above.

## Usage

### Managing connections

```bash
# Add a connection (port defaults to 5432)
pgrs add mydb --host=localhost --username=postgres --password=secret --database=mydb

# Add with a connection URL (individual flags override URL-parsed values)
pgrs add mydb --url=postgresql://user:pass@localhost:5432/mydb

# Add with custom port, TLS, and environment tag
pgrs add mydb --host=db.example.com --username=postgres --password=secret \
              --database=mydb --port=5433 --tls=require --env=production

# List all connections
pgrs list
pgrs ls

# List connection names only (useful for scripts)
pgrs list --names-only

# Edit a connection (only specified fields are updated)
pgrs edit mydb --password=newpass
pgrs edit mydb --host=db2.example.com --port=5434 --tls=verify-full

# Set or clear an environment label
pgrs edit mydb --env=staging
pgrs edit mydb --env=          # clears the label

# Rename a connection
pgrs rename mydb mydb-prod

# Test that a connection is reachable
pgrs test mydb

# Delete a connection (prompts for confirmation)
pgrs delete mydb
pgrs delete mydb --yes         # skip confirmation prompt
pgrs del mydb                  # alias
pgrs rm mydb                   # alias
```

### Connecting to a database

```bash
# Hand off to psql
pgrs connect mydb

# Open pgrs interactive SQL REPL
pgrs shell mydb
```

### Shell completions

```bash
# Bash
pgrs completions bash >> ~/.bashrc

# Zsh
pgrs completions zsh >> ~/.zshrc

# Fish
pgrs completions fish > ~/.config/fish/completions/pgrs.fish
```

### Other

```bash
pgrs --version
pgrs --help
```

## Interactive REPL (`pgrs shell`)

The built-in REPL provides tab-completion, syntax highlighting, query history, and multi-line editing.

| Command | Description |
|---------|-------------|
| `\d` | List all tables |
| `\dt` | List all tables with column count |
| `\d <table>` | Describe a table (columns, indexes, constraints) |
| `\d+ <table>` | Describe a table (extended: storage, triggers, comments) |
| `\l` | List databases |
| `\x` | Toggle expanded display |
| `\timing` | Toggle query execution time |
| `\refresh` | Reload schema (after CREATE/DROP/ALTER TABLE) |
| `\history` | Show recent query history |
| `\export <id> <path>` | Export a query result from history to a CSV file |
| `\stats` | Show most frequently queried tables |
| `\stats <table>` | Show most frequently queried columns for a table |
| `\help`, `\?` | Show REPL help |
| `\q`, `exit`, Ctrl+D | Quit |

Multi-line statements are buffered until a `;` terminates them (string literals and quoted identifiers are handled correctly).

Tab completion suggests SQL keywords, table names, and column names based on query context.

## TLS modes

| Value | Behaviour |
|-------|-----------|
| `disable` | No encryption (default) |
| `require` | Encrypt, but do not verify server certificate |
| `verify-full` | Encrypt and verify server certificate |

## Data is stored at

`~/.pgrs/pgrs.db` (SQLite — connections, query history, and usage analytics)
