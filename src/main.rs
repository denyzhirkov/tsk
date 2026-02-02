use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use rand::Rng;
use rusqlite::Connection;
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tsk")]
#[command(about = "Agent-first cli task tracker")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize tsk in current directory
    Init,
    /// Create a new task
    #[command(after_help = "Output symbols:\n  ^id  parent task\n  @id  dependency")]
    Create {
        /// Task title
        title: String,
        /// Task description
        description: String,
        /// Parent task ID (for stories/epics)
        #[arg(long)]
        parent: Option<String>,
        /// Dependency task ID (blocks completion until done)
        #[arg(long)]
        depend: Option<String>,
    },
    /// List tasks
    #[command(after_help = "Output symbols:\n  ^id  parent task\n  @id  dependency")]
    List {
        /// Show all tasks including completed
        #[arg(long)]
        all: bool,
    },
    /// Update task description
    Update {
        /// Task ID
        id: String,
        /// New description
        description: String,
    },
    /// Mark task as done (dependencies must be completed first)
    Done {
        /// Task ID
        id: String,
    },
    /// Remove a task
    Remove {
        /// Task ID
        id: String,
    },
    /// Show task details
    Show {
        /// Task ID
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

fn generate_id() -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
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
    // Check if parent_id column exists
    let has_parent: bool = conn
        .prepare("SELECT parent_id FROM tasks LIMIT 1")
        .is_ok();

    if !has_parent {
        conn.execute("ALTER TABLE tasks ADD COLUMN parent_id TEXT", [])?;
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

fn task_is_done(conn: &Connection, id: &str) -> Result<bool> {
    let done: i32 = conn.query_row(
        "SELECT done FROM tasks WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    Ok(done == 1)
}

fn cmd_init() -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let tsk_dir = current_dir.join(".tsk");

    if tsk_dir.exists() {
        println!("Already initialized.");
        return Ok(());
    }

    std::fs::create_dir_all(&tsk_dir).context("Failed to create .tsk directory")?;

    let db_path = tsk_dir.join("tsk.sqlite");
    let conn = Connection::open(&db_path).context("Failed to create database")?;
    init_db(&conn)?;

    println!("Initialized tsk in {}", tsk_dir.display());
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
        if !task_exists(conn, parent_id)? {
            bail!("Parent task '{}' not found.", parent_id);
        }
    }

    // Validate dependency exists
    if let Some(depend_id) = depend {
        if !task_exists(conn, depend_id)? {
            bail!("Dependency task '{}' not found.", depend_id);
        }
    }

    let id = generate_id();
    conn.execute(
        "INSERT INTO tasks (id, title, description, parent_id, depend_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, title, description, parent, depend],
    )?;
    println!("Created: {}", id);
    Ok(())
}

fn cmd_list(conn: &Connection, all: bool) -> Result<()> {
    let sql = if all {
        "SELECT id, title, done, parent_id, depend_id FROM tasks ORDER BY created_at"
    } else {
        "SELECT id, title, done, parent_id, depend_id FROM tasks WHERE done = 0 ORDER BY created_at"
    };

    let mut stmt = conn.prepare(sql)?;
    let tasks = stmt.query_map([], |row| {
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
        if !task_is_done(conn, &did)? {
            bail!("Cannot complete: depends on '{}' which is not done.", did);
        }
    }

    conn.execute("UPDATE tasks SET done = 1 WHERE id = ?1", [id])?;
    println!("Done: {}", id);
    Ok(())
}

fn cmd_remove(conn: &Connection, id: &str) -> Result<()> {
    let deleted = conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;

    if deleted == 0 {
        bail!("Task '{}' not found.", id);
    }

    println!("Removed: {}", id);
    Ok(())
}

fn cmd_show(conn: &Connection, id: &str) -> Result<()> {
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            cmd_init()?;
        }
        Some(cmd) => {
            let db_path = find_db_path();
            let Some(db_path) = db_path else {
                bail!("Project not initialized. Run 'tsk init' first.");
            };

            let conn = Connection::open(&db_path)?;
            migrate_db(&conn)?;

            match cmd {
                Commands::Init => unreachable!(),
                Commands::Create {
                    title,
                    description,
                    parent,
                    depend,
                } => {
                    cmd_create(&conn, &title, &description, parent.as_deref(), depend.as_deref())?;
                }
                Commands::List { all } => {
                    cmd_list(&conn, all)?;
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
            }
        }
        None => {
            // Show help when no command provided
            Cli::parse_from(["tsk", "--help"]);
        }
    }

    Ok(())
}
