CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    hash BLOB NOT NULL,
    language TEXT,
    size INTEGER NOT NULL,
    last_seen TEXT NOT NULL DEFAULT (datetime('now'))
);
