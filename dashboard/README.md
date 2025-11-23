# Neutrino Dashboard

A simple FastAPI-based dashboard for monitoring Neutrino jobs and viewing the task database.

## Features

- **Real-time job monitoring**: View all tasks with their status (pending, running, completed, failed)
- **Task statistics**: See aggregated stats including total tasks, completion rates, and average duration
- **Filtering**: Filter tasks by status and function name
- **Auto-refresh**: Optional auto-refresh every 5 seconds
- **SQLite database**: Persistent task storage with automatic schema initialization
- **REST API**: Full CRUD API for tasks

## API Endpoints

### Dashboard
- `GET /` - Web UI dashboard

### Health
- `GET /health` - Health check endpoint

### Statistics
- `GET /api/stats` - Get task statistics

### Tasks
- `GET /api/tasks` - List tasks (supports filtering and pagination)
  - Query params: `status`, `function_name`, `limit`, `offset`
- `GET /api/tasks/{task_id}` - Get specific task
- `POST /api/tasks` - Create new task (for testing)
- `DELETE /api/tasks` - Clear all tasks (for testing)

## Database Schema

```sql
CREATE TABLE tasks (
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
```

## Running Locally

```bash
cd dashboard
pip install -r requirements.txt
python app.py
```

The dashboard will be available at http://localhost:8081

## Deployment

The dashboard is automatically deployed as part of `neutrino up`:

```bash
neutrino up
```

This will:
1. Build the dashboard Docker image
2. Create a PersistentVolumeClaim for the SQLite database
3. Deploy the dashboard pod
4. Expose it via LoadBalancer on port 8081

Access the dashboard at:
- Local k3s: http://localhost:8081
- Production: http://<external-ip>:8081

## Environment Variables

- `DB_PATH` - Path to SQLite database (default: `/data/neutrino.db`)

## Tech Stack

- **FastAPI**: Web framework
- **SQLite**: Database
- **Uvicorn**: ASGI server
- **Pydantic**: Data validation
- **Vanilla JS**: Frontend (no framework needed for simplicity)
