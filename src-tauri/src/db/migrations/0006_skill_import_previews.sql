CREATE TABLE skill_import_previews (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    context_json TEXT NOT NULL CHECK(json_valid(context_json) AND json_type(context_json) = 'object'),
    redacted_preview_json TEXT NOT NULL CHECK(
        json_valid(redacted_preview_json) AND json_type(redacted_preview_json) = 'object'
    ),
    status TEXT NOT NULL DEFAULT 'previewed' CHECK(status IN ('previewed', 'consumed')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    consumed_at TEXT
);

CREATE INDEX idx_skill_import_previews_status ON skill_import_previews(status, created_at);
