#compdef tsk

_tsk_task_ids() {
    local ids
    ids=($(tsk ids 2>/dev/null))
    _describe 'task id' ids
}

_tsk() {
    local -a commands
    commands=(
        'init:Initialize tsk in current directory'
        'create:Create a new task'
        'list:List tasks'
        'show:Show task details'
        'update:Update task description'
        'start:Start working on a task'
        'done:Mark task as done'
        'remove:Remove a task'
        'completions:Generate shell completions'
    )

    _arguments -C \
        '-h[Print help]' \
        '--help[Print help]' \
        '1:command:->command' \
        '*::args:->args'

    case $state in
        command)
            _describe 'command' commands
            ;;
        args)
            case $words[1] in
                show|start|done|remove)
                    _tsk_task_ids
                    ;;
                update)
                    if [[ $CURRENT -eq 2 ]]; then
                        _tsk_task_ids
                    fi
                    ;;
                create)
                    _arguments \
                        '--parent=[Parent task ID]:task id:_tsk_task_ids' \
                        '--depend=[Dependency task ID]:task id:_tsk_task_ids' \
                        '1:title:' \
                        '2:description:'
                    ;;
                list)
                    _arguments \
                        '--inprogress[Show in progress tasks only]' \
                        '--all[Include all tasks]' \
                        '--parent=[Filter by parent task ID]:task id:_tsk_task_ids'
                    ;;
                completions)
                    _arguments '1:shell:(bash zsh fish powershell elvish)'
                    ;;
            esac
            ;;
    esac
}

compdef _tsk tsk
