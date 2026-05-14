CREATE TABLE IF NOT EXISTS structural_nodes (
    id            INTEGER PRIMARY KEY,
    file_id       INTEGER NOT NULL,
    kind          TEXT    NOT NULL,
    label         TEXT    NOT NULL,
    path_joined   TEXT    NOT NULL,
    path_json     TEXT    NOT NULL,
    byte_start    INTEGER NOT NULL,
    byte_end      INTEGER NOT NULL,
    line_start    INTEGER NOT NULL,
    line_end      INTEGER NOT NULL,
    parent_id     INTEGER,
    depth         INTEGER NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_structural_file ON structural_nodes(file_id);
CREATE INDEX IF NOT EXISTS idx_structural_path ON structural_nodes(file_id, path_joined);
CREATE INDEX IF NOT EXISTS idx_structural_parent ON structural_nodes(parent_id);

CREATE VIRTUAL TABLE IF NOT EXISTS structural_fts USING fts5(
    label,
    path_joined
);

CREATE TRIGGER IF NOT EXISTS structural_ai AFTER INSERT ON structural_nodes BEGIN
  INSERT INTO structural_fts(rowid, label, path_joined) VALUES (new.id, new.label, new.path_joined);
END;
CREATE TRIGGER IF NOT EXISTS structural_ad AFTER DELETE ON structural_nodes BEGIN
  INSERT INTO structural_fts(structural_fts, rowid, label, path_joined)
    VALUES('delete', old.id, old.label, old.path_joined);
END;
CREATE TRIGGER IF NOT EXISTS structural_au AFTER UPDATE ON structural_nodes BEGIN
  INSERT INTO structural_fts(structural_fts, rowid, label, path_joined)
    VALUES('delete', old.id, old.label, old.path_joined);
  INSERT INTO structural_fts(rowid, label, path_joined) VALUES (new.id, new.label, new.path_joined);
END;
