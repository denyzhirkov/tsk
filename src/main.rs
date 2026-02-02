use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use dialoguer::MultiSelect;
use rand::Rng;
use rusqlite::Connection;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tsk")]
#[command(about = "Agent-first cli task tracker")]
#[command(after_help = "Task ID is a 6-character code (e.g., a1b2c3) shown after create and in list output.

Example:
  tsk init
  tsk create \"Fix bug\" \"Fix login validation\"  # Created: a1b2c3
  tsk show a1b2c3
  tsk done a1b2c3")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize tsk in current directory (creates .tsk/)
    #[command(after_help = "Creates .tsk/ directory with tsk.sqlite database.
Run this once per project before using other commands.

Examples:
  tsk init                           # interactive agent selection
  tsk init --rules claude,copilot    # non-interactive install
  tsk init --rules all               # install all agent rules

Available agents: claude, copilot, cursor, windsurf")]
    Init {
        /// Install agent rules (comma-separated: claude,copilot,cursor,windsurf,all)
        #[arg(long)]
        rules: Option<String>,
    },
    /// Create a new task [--parent <id>] [--depend <id>]
    #[command(after_help = "Examples:
  tsk create \"Fix bug\" \"Fix login validation\"
  tsk create \"Subtask\" \"Details\" --parent a1b2c3
  tsk create \"Task\" \"Details\" --depend x7y8z9

Output symbols in list:
  ^id  parent task
  @id  dependency")]
    Create {
        /// Task title (short summary)
        title: String,
        /// Task description (detailed info)
        description: String,
        /// Parent task ID for subtasks/stories
        #[arg(long)]
        parent: Option<String>,
        /// Dependency: this task can't be done until depend task is done
        #[arg(long)]
        depend: Option<String>,
    },
    /// List tasks (active by default)
    #[command(after_help = "Output format:
  <id>  [status]  <title> [^parent] [@depend]

Examples:
  tsk list                  # active tasks only
  tsk list --all            # include completed
  tsk list --parent abc123  # only children of abc123")]
    List {
        /// Include completed tasks
        #[arg(long)]
        all: bool,
        /// Filter by parent task ID
        #[arg(long)]
        parent: Option<String>,
    },
    /// Update task description by ID
    #[command(after_help = "Example:
  tsk update a1b2c3 \"New detailed description\"")]
    Update {
        /// Task ID (6 chars, e.g., a1b2c3)
        id: String,
        /// New description text
        description: String,
    },
    /// Mark task as done by ID
    #[command(after_help = "Note: If task has --depend, the dependency must be completed first.")]
    Done {
        /// Task ID (6 chars, e.g., a1b2c3)
        id: String,
    },
    /// Remove task by ID
    #[command(after_help = "Cannot remove tasks that:
  - Have child tasks (--parent references this task)
  - Have active dependents (--depend references this task)")]
    Remove {
        /// Task ID (6 chars, e.g., a1b2c3)
        id: String,
    },
    /// Show full task details by ID
    #[command(after_help = "Displays: ID, title, status, parent, dependency, created date, and full description.")]
    Show {
        /// Task ID (6 chars, e.g., a1b2c3)
        id: String,
    },
    /// Generate shell completions
    #[command(hide = true)]
    Completions {
        /// Shell type
        shell: Shell,
    },
    /// List task IDs only (for shell completions)
    #[command(hide = true)]
    Ids,
}

fn find_db_path() -> Option<PathBuf> {
    let current_dir = env::current_dir().ok()?;
    let db_path = current_dir.join(".tsk").join("tsk.sqlite");
    if db_path.exists() {
        Some(db_path)
    } else {
        None
    }
}

fn generate_id(conn: &Connection) -> Result<String> {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();

    for _ in 0..100 {
        let id: String = (0..6)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();

        if !task_exists(conn, &id)? {
            return Ok(id);
        }
    }

    bail!("Failed to generate unique ID after 100 attempts");
}

fn validate_id(id: &str) -> Result<()> {
    if id.len() != 6 || !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
        bail!("Invalid task ID '{}'. Must be 6 characters [a-z0-9].", id);
    }
    Ok(())
}

fn init_db(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL,
            done INTEGER DEFAULT 0,
            parent_id TEXT,
            depend_id TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;
    Ok(())
}

fn migrate_db(conn: &Connection) -> Result<()> {
    // Check columns using PRAGMA
    let mut stmt = conn.prepare("PRAGMA table_info(tasks)")?;
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();

    if !columns.contains(&"parent_id".to_string()) {
        conn.execute("ALTER TABLE tasks ADD COLUMN parent_id TEXT", [])?;
    }
    if !columns.contains(&"depend_id".to_string()) {
        conn.execute("ALTER TABLE tasks ADD COLUMN depend_id TEXT", [])?;
    }

    Ok(())
}

fn task_exists(conn: &Connection, id: &str) -> Result<bool> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn task_is_done(conn: &Connection, id: &str) -> Result<Option<bool>> {
    let result = conn.query_row(
        "SELECT done FROM tasks WHERE id = ?1",
        [id],
        |row| row.get::<_, i32>(0),
    );

    match result {
        Ok(done) => Ok(Some(done == 1)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

const TSK_INSTRUCTIONS: &str = r#"## Task Management

This project uses `tsk` for task tracking.

### Commands
- `tsk create "<title>" "<description>"` — create task, returns ID
- `tsk create "<title>" "<desc>" --parent <id>` — create subtask
- `tsk create "<title>" "<desc>" --depend <id>` — task with dependency
- `tsk list` — show active tasks
- `tsk list --parent <id>` — show subtasks only
- `tsk show <id>` — task details
- `tsk done <id>` — mark complete
- `tsk remove <id>` — delete task

### When to use
- User asks to track/manage tasks
- Multi-step work requiring progress tracking

### Output format
`abc123  [ ]  Title ^parent @dependency`
"#;

fn install_agent_rules(current_dir: &PathBuf, agents: &[usize]) -> Result<()> {
    let agent_configs: Vec<(&str, PathBuf, bool)> = vec![
        ("Claude Code", current_dir.join("CLAUDE.md"), true),
        ("GitHub Copilot", current_dir.join(".github").join("copilot-instructions.md"), false),
        ("Cursor", current_dir.join(".cursorrules"), true),
        ("Windsurf", current_dir.join(".windsurfrules"), true),
    ];

    for &idx in agents {
        let (name, path, append) = &agent_configs[idx];

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        if *append && path.exists() {
            // Append to existing file
            let existing = fs::read_to_string(&path)?;
            if !existing.contains("## Task Management") {
                let new_content = format!("{}\n{}", existing.trim_end(), TSK_INSTRUCTIONS);
                fs::write(&path, new_content)?;
                println!("  Updated: {}", path.display());
            } else {
                println!("  Skipped: {} (already has tsk rules)", path.display());
            }
        } else {
            // Create new file
            fs::write(&path, TSK_INSTRUCTIONS)?;
            println!("  Created: {}", path.display());
        }

        let _ = name; // suppress unused warning
    }

    Ok(())
}

fn parse_rules_arg(rules: &str) -> Vec<usize> {
    let mut indices = Vec::new();
    for part in rules.to_lowercase().split(',') {
        match part.trim() {
            "all" => return vec![0, 1, 2, 3],
            "claude" => indices.push(0),
            "copilot" => indices.push(1),
            "cursor" => indices.push(2),
            "windsurf" => indices.push(3),
            _ => {}
        }
    }
    indices
}

fn cmd_init(rules: Option<&str>) -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let tsk_dir = current_dir.join(".tsk");

    let already_initialized = tsk_dir.exists();

    if !already_initialized {
        fs::create_dir_all(&tsk_dir).context("Failed to create .tsk directory")?;

        let db_path = tsk_dir.join("tsk.sqlite");
        let conn = Connection::open(&db_path).context("Failed to create database")?;
        init_db(&conn)?;

        println!("Initialized tsk in {}", tsk_dir.display());
    }

    // Handle rules installation
    if let Some(rules_str) = rules {
        // Non-interactive mode
        let selected = parse_rules_arg(rules_str);
        if !selected.is_empty() {
            if already_initialized {
                println!("Installing agent rules...");
            }
            println!();
            install_agent_rules(&current_dir, &selected)?;
            println!();
            println!("Agent rules installed.");
        }
    } else if !already_initialized {
        // Interactive mode (only for new init)
        let agents = vec![
            "Claude Code (CLAUDE.md)",
            "GitHub Copilot (.github/copilot-instructions.md)",
            "Cursor (.cursorrules)",
            "Windsurf (.windsurfrules)",
        ];

        println!();
        println!("Install AI agent rules? (Space to select, Enter to confirm)");

        let selections = MultiSelect::new()
            .items(&agents)
            .interact_opt()?;

        if let Some(selected) = selections {
            if !selected.is_empty() {
                println!();
                install_agent_rules(&current_dir, &selected)?;
                println!();
                println!("Agent rules installed.");
            }
        }
    } else {
        println!("Already initialized. Use --rules to add agent rules.");
    }

    Ok(())
}

fn cmd_create(
    conn: &Connection,
    title: &str,
    description: &str,
    parent: Option<&str>,
    depend: Option<&str>,
) -> Result<()> {
    // Validate parent exists
    if let Some(parent_id) = parent {
        validate_id(parent_id)?;
        if !task_exists(conn, parent_id)? {
            bail!("Parent task '{}' not found.", parent_id);
        }
    }

    // Validate dependency exists
    if let Some(depend_id) = depend {
        validate_id(depend_id)?;
        if !task_exists(conn, depend_id)? {
            bail!("Dependency task '{}' not found.", depend_id);
        }
    }

    let id = generate_id(conn)?;
    conn.execute(
        "INSERT INTO tasks (id, title, description, parent_id, depend_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, title, description, parent, depend],
    )?;
    println!("{}", id);
    Ok(())
}

fn cmd_list(conn: &Connection, all: bool, parent: Option<&str>) -> Result<()> {
    if let Some(pid) = parent {
        validate_id(pid)?;
        if !task_exists(conn, pid)? {
            bail!("Parent task '{}' not found.", pid);
        }
    }

    let (sql, params): (&str, Vec<&str>) = match (all, parent) {
        (true, Some(p)) => (
            "SELECT id, title, done, parent_id, depend_id FROM tasks WHERE parent_id = ?1 ORDER BY created_at",
            vec![p],
        ),
        (false, Some(p)) => (
            "SELECT id, title, done, parent_id, depend_id FROM tasks WHERE done = 0 AND parent_id = ?1 ORDER BY created_at",
            vec![p],
        ),
        (true, None) => (
            "SELECT id, title, done, parent_id, depend_id FROM tasks ORDER BY created_at",
            vec![],
        ),
        (false, None) => (
            "SELECT id, title, done, parent_id, depend_id FROM tasks WHERE done = 0 ORDER BY created_at",
            vec![],
        ),
    };

    let mut stmt = conn.prepare(sql)?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
    let tasks = stmt.query_map(params_refs.as_slice(), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;

    for task in tasks {
        let (id, title, done, parent_id, depend_id) = task?;
        let mark = if done == 1 { "x" } else { " " };

        let mut suffix = String::new();
        if let Some(pid) = parent_id {
            suffix.push_str(&format!(" ^{}", pid));
        }
        if let Some(did) = depend_id {
            suffix.push_str(&format!(" @{}", did));
        }

        println!("{}  [{}]  {}{}", id, mark, title, suffix);
    }

    Ok(())
}

fn cmd_update(conn: &Connection, id: &str, description: &str) -> Result<()> {
    validate_id(id)?;

    let updated = conn.execute(
        "UPDATE tasks SET description = ?1 WHERE id = ?2",
        [description, id],
    )?;

    if updated == 0 {
        bail!("Task '{}' not found.", id);
    }

    println!("Updated: {}", id);
    Ok(())
}

fn cmd_done(conn: &Connection, id: &str) -> Result<()> {
    validate_id(id)?;

    // Check if task exists
    if !task_exists(conn, id)? {
        bail!("Task '{}' not found.", id);
    }

    // Check if task has unfinished dependency
    let depend_id: Option<String> = conn.query_row(
        "SELECT depend_id FROM tasks WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;

    if let Some(did) = depend_id {
        match task_is_done(conn, &did)? {
            Some(true) => {} // dependency is done, OK
            Some(false) => bail!("Cannot complete: depends on '{}' which is not done.", did),
            None => {} // dependency was deleted, allow completion
        }
    }

    conn.execute("UPDATE tasks SET done = 1 WHERE id = ?1", [id])?;
    println!("Done: {}", id);
    Ok(())
}

fn cmd_remove(conn: &Connection, id: &str) -> Result<()> {
    validate_id(id)?;

    if !task_exists(conn, id)? {
        bail!("Task '{}' not found.", id);
    }

    // Check if other tasks depend on this one
    let dependents: i32 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE depend_id = ?1 AND done = 0",
        [id],
        |row| row.get(0),
    )?;

    if dependents > 0 {
        bail!("Cannot remove: {} active task(s) depend on '{}'.", dependents, id);
    }

    // Check if this task has children
    let children: i32 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE parent_id = ?1",
        [id],
        |row| row.get(0),
    )?;

    if children > 0 {
        bail!("Cannot remove: {} task(s) have '{}' as parent.", children, id);
    }

    conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
    println!("Removed: {}", id);
    Ok(())
}

fn cmd_show(conn: &Connection, id: &str) -> Result<()> {
    validate_id(id)?;

    let result = conn.query_row(
        "SELECT id, title, description, done, parent_id, depend_id, created_at FROM tasks WHERE id = ?1",
        [id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
            ))
        },
    );

    match result {
        Ok((id, title, description, done, parent_id, depend_id, created_at)) => {
            let status = if done == 1 { "done" } else { "active" };
            println!("ID:          {}", id);
            println!("Title:       {}", title);
            println!("Status:      {}", status);
            if let Some(pid) = parent_id {
                println!("Parent:      {}", pid);
            }
            if let Some(did) = depend_id {
                println!("Depends on:  {}", did);
            }
            println!("Created:     {}", created_at);
            println!();
            println!("{}", description);
            Ok(())
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            bail!("Task '{}' not found.", id);
        }
        Err(e) => Err(e.into()),
    }
}

fn cmd_ids(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("SELECT id FROM tasks WHERE done = 0")?;
    let ids = stmt.query_map([], |row| row.get::<_, String>(0))?;

    for id in ids {
        println!("{}", id?);
    }
    Ok(())
}

fn cmd_completions(shell: Shell) {
    generate(shell, &mut Cli::command(), "tsk", &mut io::stdout());
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { rules }) => {
            cmd_init(rules.as_deref())?;
        }
        Some(Commands::Completions { shell }) => {
            cmd_completions(shell);
        }
        Some(cmd) => {
            let db_path = find_db_path();
            let Some(db_path) = db_path else {
                // For ids command, just return empty if not initialized
                if matches!(cmd, Commands::Ids) {
                    return Ok(());
                }
                bail!("Project not initialized. Run 'tsk init' first.");
            };

            let conn = Connection::open(&db_path)?;
            migrate_db(&conn)?;

            match cmd {
                Commands::Init { .. } => unreachable!(),
                Commands::Completions { .. } => unreachable!(),
                Commands::Create {
                    title,
                    description,
                    parent,
                    depend,
                } => {
                    cmd_create(&conn, &title, &description, parent.as_deref(), depend.as_deref())?;
                }
                Commands::List { all, parent } => {
                    cmd_list(&conn, all, parent.as_deref())?;
                }
                Commands::Update { id, description } => {
                    cmd_update(&conn, &id, &description)?;
                }
                Commands::Done { id } => {
                    cmd_done(&conn, &id)?;
                }
                Commands::Remove { id } => {
                    cmd_remove(&conn, &id)?;
                }
                Commands::Show { id } => {
                    cmd_show(&conn, &id)?;
                }
                Commands::Ids => {
                    cmd_ids(&conn)?;
                }
            }
        }
        None => {
            // Show help when no command provided
            Cli::parse_from(["tsk", "--help"]);
        }
    }

    Ok(())
}
