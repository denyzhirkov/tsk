# tsk

Agent-first cli task tracker

## Philosophy

This project follows **Ralph's Loop** and **GSD (Get Shit Done)** principles:

- **Minimal friction** — create tasks in seconds, not minutes
- **Agent-first** — designed for AI coding assistants, not humans clicking buttons
- **No over-engineering** — SQLite file in `.tsk/`, no servers, no accounts
- **Status flow** — pending → in progress → done

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
| `tsk init` | Initialize tsk (interactive agent rules setup) |
| `tsk init --rules <agents>` | Initialize with agent rules (claude,copilot,cursor,windsurf,all) |
| `tsk create <title> <description> [--parent <id>] [--depend <id>]` | Create a new task |
| `tsk list` | List pending tasks |
| `tsk list --inprogress` | List in progress tasks |
| `tsk list --all` | List all tasks |
| `tsk list --parent <id>` | List children of a task |
| `tsk show <id>` | Show task details |
| `tsk update <id> <description>` | Update task description |
| `tsk start <id>` | Start working on a task (pending → in progress) |
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

# List pending tasks
tsk list
# abc123  [ ]  User Auth
# def456  [ ]  Login form ^abc123
# xyz789  [ ]  Validation ^abc123 @def456

# Start working on a task
tsk start def456
tsk list --inprogress
# def456  [>]  Login form ^abc123

# Complete tasks (must complete dependency first)
tsk done def456
tsk done xyz789

# View all tasks
tsk list --all
# abc123  [ ]  User Auth
# def456  [x]  Login form ^abc123
# xyz789  [x]  Validation ^abc123 @def456
```

### Output format

```
abc123  [ ]  Pending task
def456  [>]  In progress task ^abc123
xyz789  [x]  Done task ^abc123 @def456
```

- `[ ]` — pending
- `[>]` — in progress
- `[x]` — done
- `^id` — parent task
- `@id` — dependency

## AI Agent Integration

Install rules for AI coding assistants:

```bash
tsk init --rules all                 # all agents
tsk init --rules claude,copilot      # specific agents
```

Supported agents:
- **Claude Code** → `CLAUDE.md`
- **GitHub Copilot** → `.github/copilot-instructions.md`
- **Cursor** → `.cursorrules`
- **Windsurf** → `.windsurfrules`

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
