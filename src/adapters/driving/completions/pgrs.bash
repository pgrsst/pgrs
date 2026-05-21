_pgrs_completions() {
    local cur prev words cword
    _init_completion || return

    local subcommands="add list delete connect shell completions"
    local shell_names="bash zsh fish"

    if [[ $cword -eq 1 ]]; then
        COMPREPLY=($(compgen -W "$subcommands" -- "$cur"))
        return
    fi

    case "${words[1]}" in
        connect|shell|delete)
            local names
            names=$(pgrs list --names-only 2>/dev/null)
            COMPREPLY=($(compgen -W "$names" -- "$cur"))
            ;;
        completions)
            COMPREPLY=($(compgen -W "$shell_names" -- "$cur"))
            ;;
    esac
}

complete -F _pgrs_completions pgrs
