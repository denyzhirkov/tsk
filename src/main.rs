use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use dialoguer::MultiSelect;
use rand::Rng;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

mod mcp;

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
}

impl TaskStatus {
    pub fn from_int(value: i32) -> Self {
        match value {
            0 => TaskStatus::Pending,
            1 => TaskStatus::InProgress,
            _ => TaskStatus::Done,
        }
    }

    pub fn to_int(self) -> i32 {
        match self {
            TaskStatus::Pending => 0,
            TaskStatus::InProgress => 1,
            TaskStatus::Done => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Done => "done",
        }
    }

    pub fn marker(self) -> &'static str {
        match self {
            TaskStatus::Pending => " ",
            TaskStatus::InProgress => ">",
            TaskStatus::Done => "x",
        }
    }
}

/// Task data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depend_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Task summary for list output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depend_id: Option<String>,
}

/// Memory entry for storing project knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

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

    /// Update tsk to the latest version from GitHub
    #[arg(long)]
    selfupdate: bool,
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
    /// List tasks (pending by default)
    #[command(after_help = "Output format:
  <id>  [status]  <title> [^parent] [@depend]

Status symbols:
  [ ] pending
  [>] in progress
  [x] done

Examples:
  tsk list                  # pending tasks only
  tsk list --inprogress     # in progress tasks only
  tsk list --all            # all tasks
  tsk list --parent abc123  # only children of abc123")]
    List {
        /// Show in progress tasks only
        #[arg(long)]
        inprogress: bool,
        /// Include all tasks (pending, in progress, done)
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
    /// Start working on a task (mark as in progress)
    #[command(after_help = "Sets task status from pending to in_progress.
Task must be in pending status to start.")]
    Start {
        /// Task ID (6 chars, e.g., a1b2c3)
        id: String,
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
    /// Run MCP server for IDE integration
    #[command(hide = true)]
    Mcp,
    /// Store project knowledge and notes (memory)
    #[command(after_help = "Examples:
  tsk m \"API uses JWT tokens\"              # quick create
  tsk m \"Deploy via CI\" --tags deploy,ci   # with tags
  tsk m list                                # show all
  tsk m list --tag api                      # filter by tag
  tsk m show abc123                         # full details
  tsk m search \"JWT\"                        # search content
  tsk m rm abc123                           # remove")]
    M {
        #[command(subcommand)]
        action: Option<MemoryCommands>,
        /// Quick create: content text
        content: Option<String>,
        /// Tags (comma-separated)
        #[arg(long, short)]
        tags: Option<String>,
    },
}

#[derive(Subcommand)]
enum MemoryCommands {
    /// List all memory entries
    List {
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,
        /// Show only last N entries
        #[arg(long)]
        last: Option<usize>,
    },
    /// Show full memory entry
    Show {
        /// Memory ID (6 chars)
        id: String,
    },
    /// Search memories by content
    Search {
        /// Search query
        query: String,
    },
    /// Remove memory entry
    Rm {
        /// Memory ID (6 chars)
        id: String,
    },
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

fn id_exists_in_table(conn: &Connection, table: &str, id: &str) -> Result<bool> {
    let query = format!("SELECT COUNT(*) FROM {} WHERE id = ?1", table);
    let count: i32 = conn.query_row(&query, [id], |row| row.get(0))?;
    Ok(count > 0)
}

fn generate_id(conn: &Connection, table: &str) -> Result<String> {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();

    for _ in 0..100 {
        let id: String = (0..6)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();

        if !id_exists_in_table(conn, table, &id)? {
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
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            tags TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;
    Ok(())
}

fn migrate_db(conn: &Connection) -> Result<()> {
    // Ensure meta table exists (for old databases)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT
        )",
        [],
    )?;

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

    // Get current schema version
    let schema_version: i32 = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Migration v1: old done=1 → done=2 (new status model: 0=pending, 1=in_progress, 2=done)
    if schema_version < 1 {
        conn.execute("UPDATE tasks SET done = 2 WHERE done = 1", [])?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '1')",
            [],
        )?;
    }

    // Migration v2: add memories table
    if schema_version < 2 {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                tags TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '2')",
            [],
        )?;
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
        Ok(done) => Ok(Some(done == 2)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ============================================================================
// Core functions (return data, used by CLI and MCP)
// ============================================================================

/// Create a new task and return its ID
pub fn create_task(
    conn: &Connection,
    title: &str,
    description: &str,
    parent: Option<&str>,
    depend: Option<&str>,
) -> Result<String> {
    if let Some(parent_id) = parent {
        validate_id(parent_id)?;
        if !task_exists(conn, parent_id)? {
            bail!("Parent task '{}' not found.", parent_id);
        }
    }

    if let Some(depend_id) = depend {
        validate_id(depend_id)?;
        if !task_exists(conn, depend_id)? {
            bail!("Dependency task '{}' not found.", depend_id);
        }
    }

    let id = generate_id(conn, "tasks")?;
    conn.execute(
        "INSERT INTO tasks (id, title, description, parent_id, depend_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, title, description, parent, depend],
    )?;
    Ok(id)
}

/// List tasks with optional filters
pub fn list_tasks(
    conn: &Connection,
    inprogress: bool,
    all: bool,
    parent: Option<&str>,
) -> Result<Vec<TaskSummary>> {
    if let Some(pid) = parent {
        validate_id(pid)?;
        if !task_exists(conn, pid)? {
            bail!("Parent task '{}' not found.", pid);
        }
    }

    let status_filter = if all {
        None
    } else if inprogress {
        Some(1)
    } else {
        Some(0)
    };

    let (sql, params): (String, Vec<Box<dyn rusqlite::ToSql>>) = match (status_filter, parent) {
        (None, Some(p)) => (
            "SELECT id, title, done, parent_id, depend_id FROM tasks WHERE parent_id = ?1 ORDER BY created_at".to_string(),
            vec![Box::new(p.to_string()) as Box<dyn rusqlite::ToSql>],
        ),
        (Some(status), Some(p)) => (
            "SELECT id, title, done, parent_id, depend_id FROM tasks WHERE done = ?1 AND parent_id = ?2 ORDER BY created_at".to_string(),
            vec![Box::new(status) as Box<dyn rusqlite::ToSql>, Box::new(p.to_string())],
        ),
        (None, None) => (
            "SELECT id, title, done, parent_id, depend_id FROM tasks ORDER BY created_at".to_string(),
            vec![],
        ),
        (Some(status), None) => (
            "SELECT id, title, done, parent_id, depend_id FROM tasks WHERE done = ?1 ORDER BY created_at".to_string(),
            vec![Box::new(status) as Box<dyn rusqlite::ToSql>],
        ),
    };

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i32>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;

    let mut tasks = Vec::new();
    for row in rows {
        let (id, title, done, parent_id, depend_id) = row?;
        tasks.push(TaskSummary {
            id,
            title,
            status: TaskStatus::from_int(done),
            parent_id,
            depend_id,
        });
    }
    Ok(tasks)
}

/// Get full task details
pub fn get_task(conn: &Connection, id: &str) -> Result<Task> {
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
        Ok((id, title, description, done, parent_id, depend_id, created_at)) => Ok(Task {
            id,
            title,
            description,
            status: TaskStatus::from_int(done),
            parent_id,
            depend_id,
            created_at: Some(created_at),
        }),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            bail!("Task '{}' not found.", id);
        }
        Err(e) => Err(e.into()),
    }
}

/// Update task description
pub fn update_task(conn: &Connection, id: &str, description: &str) -> Result<()> {
    validate_id(id)?;

    let updated = conn.execute(
        "UPDATE tasks SET description = ?1 WHERE id = ?2",
        [description, id],
    )?;

    if updated == 0 {
        bail!("Task '{}' not found.", id);
    }
    Ok(())
}

/// Start a task (pending -> in_progress)
pub fn start_task(conn: &Connection, id: &str) -> Result<()> {
    validate_id(id)?;

    let status: i32 = conn
        .query_row("SELECT done FROM tasks WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => anyhow::anyhow!("Task '{}' not found.", id),
            _ => e.into(),
        })?;

    if status != 0 {
        if status == 1 {
            bail!("Task '{}' is already in progress.", id);
        } else {
            bail!("Task '{}' is already done.", id);
        }
    }

    conn.execute("UPDATE tasks SET done = 1 WHERE id = ?1", [id])?;
    Ok(())
}

/// Complete a task
pub fn complete_task(conn: &Connection, id: &str) -> Result<()> {
    validate_id(id)?;

    let status: i32 = conn
        .query_row("SELECT done FROM tasks WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => anyhow::anyhow!("Task '{}' not found.", id),
            _ => e.into(),
        })?;

    if status == 2 {
        bail!("Task '{}' is already done.", id);
    }

    let depend_id: Option<String> = conn.query_row(
        "SELECT depend_id FROM tasks WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;

    if let Some(did) = depend_id {
        match task_is_done(conn, &did)? {
            Some(true) => {}
            Some(false) => bail!("Cannot complete: depends on '{}' which is not done.", did),
            None => {}
        }
    }

    conn.execute("UPDATE tasks SET done = 2 WHERE id = ?1", [id])?;
    Ok(())
}

/// Remove a task
pub fn remove_task(conn: &Connection, id: &str) -> Result<()> {
    validate_id(id)?;

    if !task_exists(conn, id)? {
        bail!("Task '{}' not found.", id);
    }

    let dependents: i32 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE depend_id = ?1 AND done < 2",
        [id],
        |row| row.get(0),
    )?;

    if dependents > 0 {
        bail!(
            "Cannot remove: {} active task(s) depend on '{}'.",
            dependents,
            id
        );
    }

    let children: i32 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE parent_id = ?1",
        [id],
        |row| row.get(0),
    )?;

    if children > 0 {
        bail!("Cannot remove: {} task(s) have '{}' as parent.", children, id);
    }

    conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
    Ok(())
}

/// Get task IDs (for completions)
pub fn get_task_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM tasks WHERE done < 2")?;
    let ids = stmt.query_map([], |row| row.get::<_, String>(0))?;

    let mut result = Vec::new();
    for id in ids {
        result.push(id?);
    }
    Ok(result)
}

/// Initialize tsk in current directory (non-interactive, for MCP)
pub fn init_project() -> Result<PathBuf> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let tsk_dir = current_dir.join(".tsk");

    if !tsk_dir.exists() {
        fs::create_dir_all(&tsk_dir).context("Failed to create .tsk directory")?;
    }

    let db_path = tsk_dir.join("tsk.sqlite");
    let conn = Connection::open(&db_path).context("Failed to create database")?;
    init_db(&conn)?;

    Ok(db_path)
}

/// Find and open database, returns None if not initialized
pub fn open_db() -> Result<Option<Connection>> {
    match find_db_path() {
        Some(path) => {
            let conn = Connection::open(&path)?;
            migrate_db(&conn)?;
            Ok(Some(conn))
        }
        None => Ok(None),
    }
}

/// Find and open database, error if not initialized
pub fn require_db() -> Result<Connection> {
    match open_db()? {
        Some(conn) => Ok(conn),
        None => bail!("Project not initialized. Run 'tsk init' first."),
    }
}

// ============================================================================
// Memory core functions
// ============================================================================

fn memory_exists(conn: &Connection, id: &str) -> Result<bool> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Create a new memory entry
pub fn create_memory(conn: &Connection, content: &str, tags: Option<&str>) -> Result<String> {
    let id = generate_id(conn, "memories")?;

    conn.execute(
        "INSERT INTO memories (id, content, tags) VALUES (?1, ?2, ?3)",
        rusqlite::params![id, content, tags],
    )?;

    Ok(id)
}

/// List memory entries
pub fn list_memories(conn: &Connection, tag: Option<&str>, last: Option<usize>) -> Result<Vec<Memory>> {
    let mut memories = Vec::new();

    if let Some(t) = tag {
        let mut stmt = conn.prepare(
            "SELECT id, content, tags, created_at FROM memories WHERE tags LIKE ?1 ORDER BY created_at DESC"
        )?;
        let rows = stmt.query_map([format!("%{}%", t)], |row| {
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                tags: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        for mem in rows.flatten() {
            memories.push(mem);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, content, tags, created_at FROM memories ORDER BY created_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                tags: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;
        for mem in rows.flatten() {
            memories.push(mem);
        }
    }

    if let Some(n) = last {
        memories.truncate(n);
    }

    Ok(memories)
}

/// Get a single memory entry
pub fn get_memory(conn: &Connection, id: &str) -> Result<Memory> {
    validate_id(id)?;

    let memory = conn.query_row(
        "SELECT id, content, tags, created_at FROM memories WHERE id = ?1",
        [id],
        |row| {
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                tags: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    ).context(format!("Memory '{}' not found.", id))?;

    Ok(memory)
}

/// Search memories by content
pub fn search_memories(conn: &Connection, query: &str) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, tags, created_at FROM memories WHERE content LIKE ?1 ORDER BY created_at DESC"
    )?;

    let rows = stmt.query_map([format!("%{}%", query)], |row| {
        Ok(Memory {
            id: row.get(0)?,
            content: row.get(1)?,
            tags: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Remove a memory entry
pub fn remove_memory(conn: &Connection, id: &str) -> Result<()> {
    validate_id(id)?;

    if !memory_exists(conn, id)? {
        bail!("Memory '{}' not found.", id);
    }

    conn.execute("DELETE FROM memories WHERE id = ?1", [id])?;
    Ok(())
}

const TSK_INSTRUCTIONS: &str = r#"## Task Management

This project uses `tsk` for task tracking.

### Task Commands
- `tsk create "<title>" "<description>"` — create task, returns ID
- `tsk create "<title>" "<desc>" --parent <id>` — create subtask
- `tsk create "<title>" "<desc>" --depend <id>` — task with dependency
- `tsk list` — show pending tasks
- `tsk list --inprogress` — show in progress tasks
- `tsk list --all` — show all tasks
- `tsk list --parent <id>` — show subtasks only
- `tsk show <id>` — task details
- `tsk start <id>` — mark as in progress
- `tsk done <id>` — mark complete
- `tsk remove <id>` — delete task

### Memory Commands (project knowledge)
- `tsk m "<text>"` — store important info
- `tsk m "<text>" --tags api,auth` — store with tags
- `tsk m list` — show all memories
- `tsk m list --tag api` — filter by tag
- `tsk m search "<query>"` — search memories
- `tsk m show <id>` — show full memory
- `tsk m rm <id>` — remove memory

### When to use
- Tasks: track multi-step work, user requests task tracking
- Memory: store project decisions, important context, architecture notes

### Output format
`abc123  [ ]  Pending task ^parent @dependency`
`abc123  [>]  In progress task`
`abc123  [x]  Done task`
"#;

fn install_agent_rules(current_dir: &PathBuf, agents: &[usize]) -> Result<()> {
    let agent_configs: Vec<(&str, PathBuf)> = vec![
        ("Claude Code", current_dir.join("CLAUDE.md")),
        ("GitHub Copilot", current_dir.join(".github").join("copilot-instructions.md")),
        ("Cursor", current_dir.join(".cursorrules")),
        ("Windsurf", current_dir.join(".windsurfrules")),
    ];

    for &idx in agents {
        let (name, path) = &agent_configs[idx];

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        if path.exists() {
            // Append to existing file
            let existing = fs::read_to_string(&path)?;
            if !existing.contains("## Task Management") {
                let new_content = format!("{}\n\n{}", existing.trim_end(), TSK_INSTRUCTIONS);
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
    let id = create_task(conn, title, description, parent, depend)?;
    println!("{}", id);
    Ok(())
}

fn cmd_list(conn: &Connection, inprogress: bool, all: bool, parent: Option<&str>) -> Result<()> {
    let tasks = list_tasks(conn, inprogress, all, parent)?;

    for task in tasks {
        let mut suffix = String::new();
        if let Some(pid) = task.parent_id {
            suffix.push_str(&format!(" ^{}", pid));
        }
        if let Some(did) = task.depend_id {
            suffix.push_str(&format!(" @{}", did));
        }

        println!(
            "{}  [{}]  {}{}",
            task.id,
            task.status.marker(),
            task.title,
            suffix
        );
    }

    Ok(())
}

fn cmd_update(conn: &Connection, id: &str, description: &str) -> Result<()> {
    update_task(conn, id, description)?;
    println!("Updated: {}", id);
    Ok(())
}

fn cmd_start(conn: &Connection, id: &str) -> Result<()> {
    start_task(conn, id)?;
    println!("Started: {}", id);
    Ok(())
}

fn cmd_done(conn: &Connection, id: &str) -> Result<()> {
    complete_task(conn, id)?;
    println!("Done: {}", id);
    Ok(())
}

fn cmd_remove(conn: &Connection, id: &str) -> Result<()> {
    remove_task(conn, id)?;
    println!("Removed: {}", id);
    Ok(())
}

fn cmd_show(conn: &Connection, id: &str) -> Result<()> {
    let task = get_task(conn, id)?;

    println!("ID:          {}", task.id);
    println!("Title:       {}", task.title);
    println!("Status:      {}", task.status.as_str());
    if let Some(pid) = task.parent_id {
        println!("Parent:      {}", pid);
    }
    if let Some(did) = task.depend_id {
        println!("Depends on:  {}", did);
    }
    if let Some(created) = task.created_at {
        println!("Created:     {}", created);
    }
    println!();
    println!("{}", task.description);
    Ok(())
}

fn cmd_ids(conn: &Connection) -> Result<()> {
    let ids = get_task_ids(conn)?;
    for id in ids {
        println!("{}", id);
    }
    Ok(())
}

fn cmd_completions(shell: Shell) {
    generate(shell, &mut Cli::command(), "tsk", &mut io::stdout());
}

// ============================================================================
// Memory CLI commands
// ============================================================================

fn truncate_content(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else {
        // First 15 chars + "..." + last 5 chars
        let first: String = chars.iter().take(15).collect();
        let last: String = chars.iter().skip(chars.len().saturating_sub(5)).collect();
        format!("{}...{}", first, last)
    }
}

fn cmd_memory_create(conn: &Connection, content: &str, tags: Option<&str>) -> Result<()> {
    let id = create_memory(conn, content, tags)?;
    println!("{}", id);
    Ok(())
}

fn cmd_memory_list(conn: &Connection, tag: Option<&str>, last: Option<usize>) -> Result<()> {
    let memories = list_memories(conn, tag, last)?;

    for mem in memories {
        let content_preview = truncate_content(&mem.content, 50);
        let tags_str = mem.tags.map(|t| format!(" [{}]", t)).unwrap_or_default();
        println!("[{}] {}{}", mem.id, content_preview, tags_str);
    }

    Ok(())
}

fn cmd_memory_show(conn: &Connection, id: &str) -> Result<()> {
    let mem = get_memory(conn, id)?;

    println!("ID:      {}", mem.id);
    if let Some(tags) = mem.tags {
        println!("Tags:    {}", tags);
    }
    if let Some(created) = mem.created_at {
        println!("Created: {}", created);
    }
    println!();
    println!("{}", mem.content);
    Ok(())
}

fn cmd_memory_search(conn: &Connection, query: &str) -> Result<()> {
    let memories = search_memories(conn, query)?;

    if memories.is_empty() {
        println!("No matches found.");
        return Ok(());
    }

    for mem in memories {
        let content_preview = truncate_content(&mem.content, 50);
        let tags_str = mem.tags.map(|t| format!(" [{}]", t)).unwrap_or_default();
        println!("[{}] {}{}", mem.id, content_preview, tags_str);
    }

    Ok(())
}

fn cmd_memory_remove(conn: &Connection, id: &str) -> Result<()> {
    remove_memory(conn, id)?;
    println!("Removed: {}", id);
    Ok(())
}

const REPO: &str = "denyzhirkov/tsk";

fn cmd_selfupdate() -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: {}", current_version);
    println!("Checking for updates...");

    // Get latest version from GitHub API
    let output = Command::new("curl")
        .args(["-fsSL", &format!("https://api.github.com/repos/{}/releases/latest", REPO)])
        .output()
        .context("Failed to run curl. Is curl installed?")?;

    if !output.status.success() {
        bail!("Failed to check for updates: {}", String::from_utf8_lossy(&output.stderr));
    }

    let response: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("Failed to parse GitHub API response")?;

    let latest_tag = response["tag_name"]
        .as_str()
        .context("No tag_name in response")?;

    // Remove 'v' prefix if present
    let latest_version = latest_tag.strip_prefix('v').unwrap_or(latest_tag);

    if latest_version == current_version {
        println!("Already up to date!");
        return Ok(());
    }

    println!("New version available: {} -> {}", current_version, latest_version);
    println!("Downloading...");

    // Detect OS and architecture
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let os_name = match os {
        "macos" => "darwin",
        "linux" => "linux",
        _ => bail!("Unsupported OS: {}", os),
    };

    let arch_name = match arch {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        _ => bail!("Unsupported architecture: {}", arch),
    };

    let binary_name = format!("tsk-{}-{}", os_name, arch_name);
    let download_url = format!(
        "https://github.com/{}/releases/latest/download/{}",
        REPO, binary_name
    );

    // Get current executable path
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;

    // Download to temp file
    let temp_path = current_exe.with_extension("new");

    let status = Command::new("curl")
        .args(["-fsSL", &download_url, "-o", temp_path.to_str().unwrap()])
        .status()
        .context("Failed to download update")?;

    if !status.success() {
        bail!("Failed to download update from {}", download_url);
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&temp_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_path, perms)?;
    }

    // Replace current executable
    fs::rename(&temp_path, &current_exe)
        .context("Failed to replace executable. Try running with sudo.")?;

    println!("Updated to version {}!", latest_version);
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle --selfupdate before subcommands
    if cli.selfupdate {
        return cmd_selfupdate();
    }

    match cli.command {
        Some(Commands::Init { rules }) => {
            cmd_init(rules.as_deref())?;
        }
        Some(Commands::Completions { shell }) => {
            cmd_completions(shell);
        }
        Some(Commands::Mcp) => {
            // MCP server handles its own DB connection
            mcp::run_server()?;
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
                Commands::Mcp => unreachable!(),
                Commands::Create {
                    title,
                    description,
                    parent,
                    depend,
                } => {
                    cmd_create(&conn, &title, &description, parent.as_deref(), depend.as_deref())?;
                }
                Commands::List { inprogress, all, parent } => {
                    cmd_list(&conn, inprogress, all, parent.as_deref())?;
                }
                Commands::Update { id, description } => {
                    cmd_update(&conn, &id, &description)?;
                }
                Commands::Start { id } => {
                    cmd_start(&conn, &id)?;
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
                Commands::M { action, content, tags } => {
                    match action {
                        Some(MemoryCommands::List { tag, last }) => {
                            cmd_memory_list(&conn, tag.as_deref(), last)?;
                        }
                        Some(MemoryCommands::Show { id }) => {
                            cmd_memory_show(&conn, &id)?;
                        }
                        Some(MemoryCommands::Search { query }) => {
                            cmd_memory_search(&conn, &query)?;
                        }
                        Some(MemoryCommands::Rm { id }) => {
                            cmd_memory_remove(&conn, &id)?;
                        }
                        None => {
                            if let Some(text) = content {
                                cmd_memory_create(&conn, &text, tags.as_deref())?;
                            } else {
                                // Show help for m command
                                Cli::parse_from(["tsk", "m", "--help"]);
                            }
                        }
                    }
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
