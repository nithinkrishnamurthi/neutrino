#!/usr/bin/env python3
"""
Database migration script to add HTTP-specific columns to tasks table.
Run this to migrate from the old schema to the new gateway-compatible schema.
"""

import sqlite3
import sys

DB_PATH = "/data/neutrino.db"


def migrate_database(db_path: str = DB_PATH):
    """Add missing columns to tasks table for gateway compatibility."""
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    # Check current schema
    cursor.execute("PRAGMA table_info(tasks)")
    columns = {row[1] for row in cursor.fetchall()}

    print(f"Current columns: {columns}")

    # Add missing columns if they don't exist
    migrations = [
        ("method", "ALTER TABLE tasks ADD COLUMN method TEXT"),
        ("path", "ALTER TABLE tasks ADD COLUMN path TEXT"),
        ("status_code", "ALTER TABLE tasks ADD COLUMN status_code INTEGER"),
        ("request_body", "ALTER TABLE tasks ADD COLUMN request_body TEXT"),
        ("response_body", "ALTER TABLE tasks ADD COLUMN response_body TEXT"),
    ]

    for column_name, sql in migrations:
        if column_name not in columns:
            print(f"Adding column: {column_name}")
            try:
                cursor.execute(sql)
                print(f"  ✓ Added {column_name}")
            except sqlite3.Error as e:
                print(f"  ✗ Failed to add {column_name}: {e}")
        else:
            print(f"Column {column_name} already exists, skipping")

    # Create new indexes
    indexes = [
        ("idx_method", "CREATE INDEX IF NOT EXISTS idx_method ON tasks(method)"),
        ("idx_status_code", "CREATE INDEX IF NOT EXISTS idx_status_code ON tasks(status_code)"),
    ]

    for index_name, sql in indexes:
        print(f"Creating index: {index_name}")
        try:
            cursor.execute(sql)
            print(f"  ✓ Created {index_name}")
        except sqlite3.Error as e:
            print(f"  ✗ Failed to create {index_name}: {e}")

    conn.commit()
    conn.close()

    print("\n✓ Migration complete!")


if __name__ == "__main__":
    db_path = sys.argv[1] if len(sys.argv) > 1 else DB_PATH
    print(f"Migrating database: {db_path}")
    migrate_database(db_path)
