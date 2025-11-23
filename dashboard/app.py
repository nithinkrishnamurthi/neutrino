"""
Neutrino Dashboard - Job monitoring and database viewer
"""

from datetime import datetime
from typing import List, Optional
from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException, Query
from fastapi.responses import HTMLResponse
from pydantic import BaseModel
import sqlite3
import json


# Database models
class TaskStatus(str):
    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


class Task(BaseModel):
    id: str
    function_name: str
    status: str
    created_at: datetime
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    worker_id: Optional[str] = None
    args: Optional[dict] = None
    result: Optional[dict] = None
    error: Optional[str] = None
    duration_ms: Optional[float] = None


class TaskCreate(BaseModel):
    function_name: str
    args: dict = {}


class TaskStats(BaseModel):
    total: int
    pending: int
    running: int
    completed: int
    failed: int
    cancelled: int
    avg_duration_ms: Optional[float] = None


# Database connection
DB_PATH = "/data/neutrino.db"


def get_db():
    """Get database connection."""
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    return conn


def init_db():
    """Initialize database schema."""
    conn = get_db()
    cursor = conn.cursor()

    # Create tasks table
    cursor.execute("""
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            function_name TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            started_at TIMESTAMP,
            completed_at TIMESTAMP,
            worker_id TEXT,
            args TEXT,
            result TEXT,
            error TEXT,
            duration_ms REAL
        )
    """)

    # Create indexes
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_status ON tasks(status)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_created_at ON tasks(created_at)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_function_name ON tasks(function_name)")

    conn.commit()
    conn.close()


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Initialize database on startup."""
    init_db()
    yield


# FastAPI app
app = FastAPI(
    title="Neutrino Dashboard",
    description="Job monitoring and database viewer for Neutrino",
    version="0.1.0",
    lifespan=lifespan
)


@app.get("/", response_class=HTMLResponse)
async def root():
    """Dashboard home page."""
    return """
    <!DOCTYPE html>
    <html>
    <head>
        <title>Neutrino Dashboard</title>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <style>
            * { margin: 0; padding: 0; box-sizing: border-box; }
            body {
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                background: #0a0e27;
                color: #e4e4e7;
                padding: 2rem;
            }
            .container { max-width: 1400px; margin: 0 auto; }
            h1 {
                font-size: 2.5rem;
                margin-bottom: 0.5rem;
                background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
                -webkit-background-clip: text;
                -webkit-text-fill-color: transparent;
            }
            .subtitle { color: #a1a1aa; margin-bottom: 2rem; }
            .stats-grid {
                display: grid;
                grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
                gap: 1rem;
                margin-bottom: 2rem;
            }
            .stat-card {
                background: #1a1f3a;
                padding: 1.5rem;
                border-radius: 0.5rem;
                border: 1px solid #27293d;
            }
            .stat-label { color: #a1a1aa; font-size: 0.875rem; margin-bottom: 0.5rem; }
            .stat-value { font-size: 2rem; font-weight: bold; }
            .stat-value.pending { color: #fbbf24; }
            .stat-value.running { color: #3b82f6; }
            .stat-value.completed { color: #10b981; }
            .stat-value.failed { color: #ef4444; }
            .controls {
                display: flex;
                gap: 1rem;
                margin-bottom: 1rem;
                align-items: center;
            }
            button {
                background: #667eea;
                color: white;
                border: none;
                padding: 0.5rem 1rem;
                border-radius: 0.375rem;
                cursor: pointer;
                font-size: 0.875rem;
                font-weight: 500;
            }
            button:hover { background: #5568d3; }
            button.secondary { background: #374151; }
            button.secondary:hover { background: #4b5563; }
            select, input {
                background: #1a1f3a;
                color: #e4e4e7;
                border: 1px solid #27293d;
                padding: 0.5rem;
                border-radius: 0.375rem;
                font-size: 0.875rem;
            }
            .tasks-table {
                background: #1a1f3a;
                border-radius: 0.5rem;
                border: 1px solid #27293d;
                overflow: hidden;
            }
            table { width: 100%; border-collapse: collapse; }
            th {
                background: #0f1220;
                padding: 1rem;
                text-align: left;
                font-weight: 600;
                font-size: 0.875rem;
                color: #a1a1aa;
                text-transform: uppercase;
                letter-spacing: 0.05em;
            }
            td {
                padding: 1rem;
                border-top: 1px solid #27293d;
                font-size: 0.875rem;
            }
            tr:hover { background: #1e2339; }
            .badge {
                display: inline-block;
                padding: 0.25rem 0.5rem;
                border-radius: 0.25rem;
                font-size: 0.75rem;
                font-weight: 600;
                text-transform: uppercase;
            }
            .badge.pending { background: #fbbf2420; color: #fbbf24; }
            .badge.running { background: #3b82f620; color: #3b82f6; }
            .badge.completed { background: #10b98120; color: #10b981; }
            .badge.failed { background: #ef444420; color: #ef4444; }
            .badge.cancelled { background: #6b728020; color: #6b7280; }
            .code {
                font-family: monospace;
                background: #0f1220;
                padding: 0.25rem 0.5rem;
                border-radius: 0.25rem;
                font-size: 0.75rem;
            }
            .error { max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
        </style>
    </head>
    <body>
        <div class="container">
            <h1>⚡ Neutrino Dashboard</h1>
            <p class="subtitle">Job monitoring and database viewer</p>

            <div class="stats-grid" id="stats">
                <div class="stat-card">
                    <div class="stat-label">Total Tasks</div>
                    <div class="stat-value">-</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Pending</div>
                    <div class="stat-value pending">-</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Running</div>
                    <div class="stat-value running">-</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Completed</div>
                    <div class="stat-value completed">-</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Failed</div>
                    <div class="stat-value failed">-</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Avg Duration</div>
                    <div class="stat-value">- ms</div>
                </div>
            </div>

            <div class="controls">
                <button onclick="loadTasks()">🔄 Refresh</button>
                <select id="statusFilter" onchange="loadTasks()">
                    <option value="">All Statuses</option>
                    <option value="pending">Pending</option>
                    <option value="running">Running</option>
                    <option value="completed">Completed</option>
                    <option value="failed">Failed</option>
                    <option value="cancelled">Cancelled</option>
                </select>
                <input type="text" id="functionFilter" placeholder="Filter by function..." onchange="loadTasks()">
                <label style="margin-left: auto;">
                    <input type="checkbox" id="autoRefresh" onchange="toggleAutoRefresh()"> Auto-refresh (5s)
                </label>
            </div>

            <div class="tasks-table">
                <table>
                    <thead>
                        <tr>
                            <th>ID</th>
                            <th>Function</th>
                            <th>Status</th>
                            <th>Worker</th>
                            <th>Created</th>
                            <th>Duration</th>
                            <th>Error</th>
                        </tr>
                    </thead>
                    <tbody id="tasksBody">
                        <tr><td colspan="7" style="text-align: center; padding: 2rem;">Loading...</td></tr>
                    </tbody>
                </table>
            </div>
        </div>

        <script>
            let autoRefreshInterval = null;

            async function loadStats() {
                try {
                    const response = await fetch('/api/stats');
                    const stats = await response.json();

                    const cards = document.querySelectorAll('.stat-card');
                    cards[0].querySelector('.stat-value').textContent = stats.total;
                    cards[1].querySelector('.stat-value').textContent = stats.pending;
                    cards[2].querySelector('.stat-value').textContent = stats.running;
                    cards[3].querySelector('.stat-value').textContent = stats.completed;
                    cards[4].querySelector('.stat-value').textContent = stats.failed;
                    cards[5].querySelector('.stat-value').textContent =
                        stats.avg_duration_ms ? `${stats.avg_duration_ms.toFixed(2)} ms` : '- ms';
                } catch (error) {
                    console.error('Failed to load stats:', error);
                }
            }

            async function loadTasks() {
                const status = document.getElementById('statusFilter').value;
                const functionName = document.getElementById('functionFilter').value;

                let url = '/api/tasks?limit=100';
                if (status) url += `&status=${status}`;
                if (functionName) url += `&function_name=${functionName}`;

                try {
                    const response = await fetch(url);
                    const tasks = await response.json();

                    const tbody = document.getElementById('tasksBody');
                    if (tasks.length === 0) {
                        tbody.innerHTML = '<tr><td colspan="7" style="text-align: center; padding: 2rem;">No tasks found</td></tr>';
                        return;
                    }

                    tbody.innerHTML = tasks.map(task => `
                        <tr>
                            <td><span class="code">${task.id.substring(0, 8)}</span></td>
                            <td>${task.function_name}</td>
                            <td><span class="badge ${task.status}">${task.status}</span></td>
                            <td>${task.worker_id ? '<span class="code">' + task.worker_id.substring(0, 8) + '</span>' : '-'}</td>
                            <td>${new Date(task.created_at).toLocaleString()}</td>
                            <td>${task.duration_ms ? task.duration_ms.toFixed(2) + ' ms' : '-'}</td>
                            <td class="error">${task.error || '-'}</td>
                        </tr>
                    `).join('');

                    await loadStats();
                } catch (error) {
                    console.error('Failed to load tasks:', error);
                    document.getElementById('tasksBody').innerHTML =
                        '<tr><td colspan="7" style="text-align: center; padding: 2rem; color: #ef4444;">Error loading tasks</td></tr>';
                }
            }

            function toggleAutoRefresh() {
                const enabled = document.getElementById('autoRefresh').checked;
                if (enabled) {
                    autoRefreshInterval = setInterval(loadTasks, 5000);
                } else {
                    clearInterval(autoRefreshInterval);
                }
            }

            // Load tasks on page load
            loadTasks();
        </script>
    </body>
    </html>
    """


@app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy"}


@app.get("/api/stats", response_model=TaskStats)
async def get_stats():
    """Get task statistics."""
    conn = get_db()
    cursor = conn.cursor()

    # Get counts by status
    cursor.execute("""
        SELECT
            COUNT(*) as total,
            SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) as pending,
            SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END) as running,
            SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as completed,
            SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed,
            SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END) as cancelled,
            AVG(CASE WHEN duration_ms IS NOT NULL THEN duration_ms END) as avg_duration_ms
        FROM tasks
    """)

    row = cursor.fetchone()
    conn.close()

    return TaskStats(
        total=row["total"],
        pending=row["pending"] or 0,
        running=row["running"] or 0,
        completed=row["completed"] or 0,
        failed=row["failed"] or 0,
        cancelled=row["cancelled"] or 0,
        avg_duration_ms=row["avg_duration_ms"]
    )


@app.get("/api/tasks", response_model=List[Task])
async def get_tasks(
    status: Optional[str] = Query(None, description="Filter by status"),
    function_name: Optional[str] = Query(None, description="Filter by function name"),
    limit: int = Query(100, ge=1, le=1000, description="Maximum number of tasks to return"),
    offset: int = Query(0, ge=0, description="Number of tasks to skip")
):
    """Get list of tasks."""
    conn = get_db()
    cursor = conn.cursor()

    query = "SELECT * FROM tasks WHERE 1=1"
    params = []

    if status:
        query += " AND status = ?"
        params.append(status)

    if function_name:
        query += " AND function_name LIKE ?"
        params.append(f"%{function_name}%")

    query += " ORDER BY created_at DESC LIMIT ? OFFSET ?"
    params.extend([limit, offset])

    cursor.execute(query, params)
    rows = cursor.fetchall()
    conn.close()

    tasks = []
    for row in rows:
        task_dict = dict(row)
        # Parse JSON fields
        if task_dict.get("args"):
            task_dict["args"] = json.loads(task_dict["args"])
        if task_dict.get("result"):
            task_dict["result"] = json.loads(task_dict["result"])
        tasks.append(Task(**task_dict))

    return tasks


@app.get("/api/tasks/{task_id}", response_model=Task)
async def get_task(task_id: str):
    """Get a specific task by ID."""
    conn = get_db()
    cursor = conn.cursor()

    cursor.execute("SELECT * FROM tasks WHERE id = ?", (task_id,))
    row = cursor.fetchone()
    conn.close()

    if not row:
        raise HTTPException(status_code=404, detail="Task not found")

    task_dict = dict(row)
    if task_dict.get("args"):
        task_dict["args"] = json.loads(task_dict["args"])
    if task_dict.get("result"):
        task_dict["result"] = json.loads(task_dict["result"])

    return Task(**task_dict)


@app.post("/api/tasks", response_model=Task)
async def create_task(task: TaskCreate):
    """Create a new task (for testing purposes)."""
    import uuid
    from datetime import datetime

    task_id = str(uuid.uuid4())
    conn = get_db()
    cursor = conn.cursor()

    cursor.execute("""
        INSERT INTO tasks (id, function_name, status, args)
        VALUES (?, ?, ?, ?)
    """, (task_id, task.function_name, TaskStatus.PENDING, json.dumps(task.args)))

    conn.commit()
    conn.close()

    return await get_task(task_id)


@app.delete("/api/tasks")
async def clear_tasks():
    """Clear all tasks (for testing purposes)."""
    conn = get_db()
    cursor = conn.cursor()
    cursor.execute("DELETE FROM tasks")
    deleted = cursor.rowcount
    conn.commit()
    conn.close()

    return {"deleted": deleted, "message": f"Deleted {deleted} tasks"}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8081)
