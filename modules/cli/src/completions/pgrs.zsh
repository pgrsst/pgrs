#compdef pgrs
# To install, add this line to ~/.zshrc:
#   eval "$(pgrs completions zsh)"

_pgrs() {
    local state

    _arguments \
        '1: :->subcommand' \
        '*: :->args'

    case $state in
        subcommand)
            local subcommands
            subcommands=(add list delete connect shell completions)
            _describe 'subcommand' subcommands
            ;;
        args)
            case ${words[2]} in
                connect|shell|delete)
                    local names
                    names=(${(f)"$(pgrs list --names-only 2>/dev/null)"})
                    _describe 'connection' names
                    ;;
                completions)
                    local shells
                    shells=(bash zsh fish)
                    _describe 'shell' shells
                    ;;
            esac
            ;;
    esac
}

_pgrs
