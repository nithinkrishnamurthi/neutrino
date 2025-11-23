use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub function_name: Option<String>,
    pub method: String,
    pub path: String,
    pub status: String,
    pub created_at: Option<String>,
    pub completed_at: Option<String>,
    pub duration_ms: Option<f64>,
    pub status_code: Option<u16>,
    pub request_body: Option<String>,
    pub response_body: Option<String>,
    pub error: Option<String>,
}

impl Default for LogEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            function_name: None,
            method: String::new(),
            path: String::new(),
            status: String::new(),
            created_at: None,
            completed_at: None,
            duration_ms: None,
            status_code: None,
            request_body: None,
            response_body: None,
            error: None,
        }
    }
}

/// Non-blocking database logger with retry logic
pub struct DbLogger {
    sender: mpsc::UnboundedSender<LogEntry>,
}

impl DbLogger {
    /// Create a new database logger
    /// Spawns a background task that processes log entries
    pub fn new(db_path: String) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn background writer task
        tokio::spawn(async move {
            db_writer_task(rx, db_path).await;
        });

        Self { sender: tx }
    }

    /// Log a request (non-blocking)
    /// If the channel is closed, this will silently fail
    pub fn log(&self, entry: LogEntry) {
        if let Err(e) = self.sender.send(entry) {
            error!("Failed to send log entry to background task: {}", e);
        }
    }
}

/// Background task that processes log entries with retry logic
async fn db_writer_task(mut rx: mpsc::UnboundedReceiver<LogEntry>, db_path: String) {
    info!("Database writer task started");

    // Initialize database
    if let Err(e) = init_database(&db_path) {
        error!("Failed to initialize database: {}", e);
        return;
    }

    while let Some(entry) = rx.recv().await {
        // Retry up to 3 times with exponential backoff
        let mut success = false;
        for attempt in 0..3 {
            match write_log_entry(&db_path, &entry) {
                Ok(_) => {
                    success = true;
                    break;
                }
                Err(e) => {
                    if attempt < 2 {
                        let backoff_ms = 100 * 2_u64.pow(attempt);
                        warn!(
                            "Failed to write log entry (attempt {}/3): {}. Retrying in {}ms",
                            attempt + 1,
                            e,
                            backoff_ms
                        );
                        sleep(Duration::from_millis(backoff_ms)).await;
                    } else {
                        error!(
                            "Failed to write log entry after 3 attempts: {}. Entry ID: {}",
                            e, entry.id
                        );
                    }
                }
            }
        }

        if !success {
            warn!("Giving up on log entry: {}", entry.id);
        }
    }

    info!("Database writer task stopped");
}

/// Initialize database schema
fn init_database(db_path: &str) -> rusqlite::Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = Path::new(db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let conn = Connection::open(db_path)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            function_name TEXT,
            method TEXT NOT NULL,
            path TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            completed_at TIMESTAMP,
            duration_ms REAL,
            status_code INTEGER,
            request_body TEXT,
            response_body TEXT,
            error TEXT
        )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_status ON tasks(status)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_created_at ON tasks(created_at)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_function_name ON tasks(function_name)",
        [],
    )?;

    info!("Database initialized successfully at: {}", db_path);
    Ok(())
}

/// Write a log entry to the database
fn write_log_entry(db_path: &str, entry: &LogEntry) -> rusqlite::Result<()> {
    let conn = Connection::open(db_path)?;

    // Use INSERT OR REPLACE to handle both new entries and updates
    conn.execute(
        "INSERT OR REPLACE INTO tasks (
            id, function_name, method, path, status, created_at, completed_at,
            duration_ms, status_code, request_body, response_body, error
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            entry.id,
            entry.function_name,
            entry.method,
            entry.path,
            entry.status,
            entry.created_at,
            entry.completed_at,
            entry.duration_ms,
            entry.status_code,
            entry.request_body,
            entry.response_body,
            entry.error,
        ],
    )?;

    Ok(())
}
