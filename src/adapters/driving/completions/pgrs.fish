# To install, run:
#   pgrs completions fish > ~/.config/fish/completions/pgrs.fish

function __pgrs_connections
    pgrs list --names-only 2>/dev/null
end

complete -c pgrs -f -n '__fish_use_subcommand' -a add         -d 'Add a new connection'
complete -c pgrs -f -n '__fish_use_subcommand' -a list        -d 'List all connections'
complete -c pgrs -f -n '__fish_use_subcommand' -a delete      -d 'Delete a connection'
complete -c pgrs -f -n '__fish_use_subcommand' -a connect     -d 'Open psql session'
complete -c pgrs -f -n '__fish_use_subcommand' -a shell       -d 'Open pgrs interactive REPL'
complete -c pgrs -f -n '__fish_use_subcommand' -a completions -d 'Print shell completion script'

complete -c pgrs -f -n '__fish_seen_subcommand_from connect shell delete' -a '(__pgrs_connections)'
complete -c pgrs -f -n '__fish_seen_subcommand_from completions' -a 'bash zsh fish' -d 'Shell type'
