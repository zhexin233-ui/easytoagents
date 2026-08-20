CREATE TABLE profile_import_previews (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    artifact_kind TEXT NOT NULL CHECK(artifact_kind IN ('provider', 'prompt')),
    target_path TEXT NOT NULL CHECK(
        target_path LIKE '/%' AND target_path != '/' AND instr(target_path, '//') = 0
        AND target_path NOT LIKE '%/../%' AND target_path NOT LIKE '%/./%'
        AND target_path NOT LIKE '%/..' AND target_path NOT LIKE '%/.'
        AND substr(target_path, -1) != '/'
    ),
    observed_full_hash TEXT NOT NULL CHECK(
        length(observed_full_hash) = 64 AND observed_full_hash = lower(observed_full_hash)
        AND observed_full_hash NOT GLOB '*[^0-9a-f]*'
    ),
    suggested_name TEXT NOT NULL CHECK(
        length(suggested_name) BETWEEN 1 AND 100 AND suggested_name = trim(suggested_name)
    ),
    redacted_preview_json TEXT NOT NULL CHECK(
        json_valid(redacted_preview_json) AND json_type(redacted_preview_json) = 'object'
    ),
    status TEXT NOT NULL DEFAULT 'previewed' CHECK(status IN ('previewed', 'consumed')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    consumed_at TEXT
);

CREATE INDEX idx_profile_import_previews_status
    ON profile_import_previews(status, created_at);
