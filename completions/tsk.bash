_tsk() {
    local cur prev words cword
    _init_completion || return

    local commands="init create list show update done remove completions"

    if [[ $cword -eq 1 ]]; then
        COMPREPLY=($(compgen -W "$commands" -- "$cur"))
        return
    fi

    local cmd="${words[1]}"

    case $cmd in
        show|done|remove)
            if [[ $cword -eq 2 ]]; then
                local ids=$(tsk ids 2>/dev/null)
                COMPREPLY=($(compgen -W "$ids" -- "$cur"))
            fi
            ;;
        update)
            if [[ $cword -eq 2 ]]; then
                local ids=$(tsk ids 2>/dev/null)
                COMPREPLY=($(compgen -W "$ids" -- "$cur"))
            fi
            ;;
        create)
            case $prev in
                --parent|--depend)
                    local ids=$(tsk ids 2>/dev/null)
                    COMPREPLY=($(compgen -W "$ids" -- "$cur"))
                    ;;
                *)
                    if [[ $cur == -* ]]; then
                        COMPREPLY=($(compgen -W "--parent --depend" -- "$cur"))
                    fi
                    ;;
            esac
            ;;
        list)
            if [[ $cur == -* ]]; then
                COMPREPLY=($(compgen -W "--all" -- "$cur"))
            fi
            ;;
        completions)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=($(compgen -W "bash zsh fish powershell elvish" -- "$cur"))
            fi
            ;;
    esac
}

complete -F _tsk tsk
