#compdef tsk

_tsk_task_ids() {
    local ids
    ids=($(tsk ids 2>/dev/null))
    _describe 'task id' ids
}

_tsk_memory_ids() {
    local ids
    ids=($(tsk m list 2>/dev/null | grep -oE '^\[[a-z0-9]{6}\]' | tr -d '[]'))
    _describe 'memory id' ids
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
        'm:Store project knowledge (memory)'
        'completions:Generate shell completions'
    )

    _arguments -C \
        '-h[Print help]' \
        '--help[Print help]' \
        '--selfupdate[Update tsk to latest version]' \
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
                m)
                    local -a m_commands
                    m_commands=(
                        'list:List memory entries'
                        'show:Show memory entry'
                        'search:Search memories'
                        'rm:Remove memory entry'
                    )
                    _arguments -C \
                        '-t[Tags]:tags:' \
                        '--tags=[Tags]:tags:' \
                        '1:subcommand:->m_cmd' \
                        '*::args:->m_args'
                    case $state in
                        m_cmd)
                            _describe 'memory command' m_commands
                            ;;
                        m_args)
                            case $words[1] in
                                show|rm)
                                    _tsk_memory_ids
                                    ;;
                                list)
                                    _arguments \
                                        '--tag=[Filter by tag]:tag:' \
                                        '--last=[Show last N entries]:number:'
                                    ;;
                            esac
                            ;;
                    esac
                    ;;
            esac
            ;;
    esac
}

compdef _tsk tsk
