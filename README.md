# tsk

Agent-first cli task tracker

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/denyzhirkov/tsk/master/install.sh | sh
```


## Usage

Initialize tsk in your project directory:

```bash
cd your-project
tsk init
```

This creates `.tsk/tsk.sqlite` in the current directory.

### Commands

| Command | Description |
|---------|-------------|
| `tsk init` | Initialize tsk in current directory |
| `tsk create <title> <description> [--parent <id>] [--depend <id>]` | Create a new task |
| `tsk list` | List active tasks |
| `tsk list --all` | List all tasks including completed |
| `tsk show <id>` | Show task details |
| `tsk update <id> <description>` | Update task description |
| `tsk done <id>` | Mark task as done |
| `tsk remove <id>` | Remove a task |

### Create options

- `--parent <id>` — set parent task (for stories/epics)
- `--depend <id>` — set dependency (must be completed before this task can be done)

### Example

```bash
tsk init

# Create a story
tsk create "User Auth" "Implement authentication"      # Created: abc123

# Create subtasks
tsk create "Login form" "Create form" --parent abc123  # Created: def456
tsk create "Validation" "Add validation" --parent abc123 --depend def456

# List tasks
tsk list
# abc123  [ ]  User Auth
# def456  [ ]  Login form ^abc123
# xyz789  [ ]  Validation ^abc123 @def456

# Complete tasks (must complete dependency first)
tsk done def456
tsk done xyz789
```

### Output format

```
abc123  [ ]  User Auth
def456  [ ]  Login form ^abc123
xyz789  [x]  Validation ^abc123 @def456
```

- `^id` — parent task
- `@id` — dependency

## Tab completion

Tab completion is installed automatically. Restart terminal after install.

Manual setup:
```bash
# zsh
source <(tsk completions zsh)

# bash
source <(tsk completions bash)
```

Supports completing task IDs: `tsk show [TAB]`
