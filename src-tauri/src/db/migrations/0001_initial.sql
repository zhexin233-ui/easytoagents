CREATE TABLE provider_profiles (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    name TEXT NOT NULL COLLATE NOCASE
        CHECK(
            length(name) BETWEEN 1 AND 100 AND name = trim(name)
            AND instr(name, char(0)) = 0
            AND name NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || '-' || char(159) || ']*')
        ),
    api_base_url TEXT,
    api_key TEXT,
    default_model TEXT,
    config_json TEXT NOT NULL DEFAULT '{}'
        CHECK(json_valid(config_json) AND json_type(config_json) = 'object'),
    is_active INTEGER NOT NULL DEFAULT 0 CHECK(is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1),
    UNIQUE(tool, name)
);

CREATE UNIQUE INDEX uq_provider_profiles_one_active_per_tool
    ON provider_profiles(tool) WHERE is_active = 1;

CREATE TABLE prompt_profiles (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    name TEXT NOT NULL COLLATE NOCASE
        CHECK(
            length(name) BETWEEN 1 AND 100 AND name = trim(name)
            AND instr(name, char(0)) = 0
            AND name NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || '-' || char(159) || ']*')
        ),
    body TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 0 CHECK(is_active IN (0, 1)),
    imported_from_path TEXT CHECK(
        imported_from_path IS NULL OR (
            imported_from_path LIKE '/%'
            AND imported_from_path != '/'
            AND instr(imported_from_path, '//') = 0
            AND imported_from_path NOT LIKE '%/../%'
            AND imported_from_path NOT LIKE '%/./%'
            AND imported_from_path NOT LIKE '%/..'
            AND imported_from_path NOT LIKE '%/.'
            AND substr(imported_from_path, -1) != '/'
        )
    ),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1),
    UNIQUE(tool, name)
);

CREATE UNIQUE INDEX uq_prompt_profiles_one_active_per_tool
    ON prompt_profiles(tool) WHERE is_active = 1;

CREATE TABLE mcp_servers (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    name TEXT NOT NULL COLLATE NOCASE UNIQUE
        CHECK(
            length(name) BETWEEN 1 AND 100 AND name = trim(name)
            AND instr(name, char(0)) = 0
            AND name NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || '-' || char(159) || ']*')
        ),
    transport TEXT NOT NULL CHECK(transport IN ('stdio', 'streamable_http')),
    command TEXT,
    args_json TEXT NOT NULL DEFAULT '[]'
        CHECK(json_valid(args_json) AND json_type(args_json) = 'array'),
    url TEXT,
    headers_json TEXT NOT NULL DEFAULT '{}'
        CHECK(json_valid(headers_json) AND json_type(headers_json) = 'object'),
    env_json TEXT NOT NULL DEFAULT '{}'
        CHECK(json_valid(env_json) AND json_type(env_json) = 'object'),
    extra_json TEXT NOT NULL DEFAULT '{}'
        CHECK(json_valid(extra_json) AND json_type(extra_json) = 'object'),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1),
    CHECK(
        (transport = 'stdio' AND command IS NOT NULL AND length(trim(command)) > 0 AND url IS NULL)
        OR
        (transport = 'streamable_http' AND url IS NOT NULL AND length(trim(url)) > 0 AND command IS NULL)
    )
);

CREATE TABLE skills (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    name TEXT NOT NULL COLLATE NOCASE UNIQUE
        CHECK(
            length(name) BETWEEN 1 AND 100 AND name = trim(name)
            AND instr(name, char(0)) = 0
            AND name NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || '-' || char(159) || ']*')
        ),
    source_path TEXT NOT NULL CHECK(
        source_path LIKE '/%' AND source_path != '/' AND instr(source_path, '//') = 0
        AND source_path NOT LIKE '%/../%' AND source_path NOT LIKE '%/./%'
        AND source_path NOT LIKE '%/..' AND source_path NOT LIKE '%/.'
        AND substr(source_path, -1) != '/'
    ),
    central_path TEXT NOT NULL UNIQUE CHECK(
        central_path LIKE '/%' AND central_path != '/' AND instr(central_path, '//') = 0
        AND central_path NOT LIKE '%/../%' AND central_path NOT LIKE '%/./%'
        AND central_path NOT LIKE '%/..' AND central_path NOT LIKE '%/.'
        AND substr(central_path, -1) != '/'
    ),
    content_hash TEXT NOT NULL CHECK(
        length(content_hash) = 64 AND content_hash = lower(content_hash)
        AND content_hash NOT GLOB '*[^0-9a-f]*'
    ),
    frontmatter_json TEXT NOT NULL DEFAULT '{}'
        CHECK(json_valid(frontmatter_json) AND json_type(frontmatter_json) = 'object'),
    status TEXT NOT NULL DEFAULT 'ready' CHECK(status IN ('ready', 'invalid', 'missing')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1)
);

CREATE TABLE projects (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    display_name TEXT NOT NULL
        CHECK(
            length(display_name) BETWEEN 1 AND 100 AND display_name = trim(display_name)
            AND instr(display_name, char(0)) = 0
            AND display_name NOT GLOB ('*[' || char(1) || '-' || char(31) || char(127) || '-' || char(159) || ']*')
        ),
    root_path TEXT NOT NULL UNIQUE CHECK(
        root_path LIKE '/%'
        AND instr(root_path, '//') = 0
        AND root_path NOT LIKE '%/../%'
        AND root_path NOT LIKE '%/./%'
        AND root_path NOT LIKE '%/..'
        AND root_path NOT LIKE '%/.'
        AND root_path != '/'
        AND substr(root_path, -1) != '/'
    ),
    is_git_repo INTEGER NOT NULL DEFAULT 0 CHECK(is_git_repo IN (0, 1)),
    codex_trust_status TEXT NOT NULL DEFAULT 'unknown'
        CHECK(codex_trust_status IN ('unknown', 'trusted', 'untrusted')),
    last_scanned_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1)
);

CREATE TABLE mcp_global_assignments (
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    mcp_id TEXT NOT NULL REFERENCES mcp_servers(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(tool, mcp_id)
);

CREATE TABLE skill_global_assignments (
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    skill_id TEXT NOT NULL REFERENCES skills(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(tool, skill_id)
);

CREATE TABLE mcp_project_assignments (
    project_id TEXT NOT NULL REFERENCES projects(id) ON UPDATE CASCADE ON DELETE CASCADE,
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    mcp_id TEXT NOT NULL REFERENCES mcp_servers(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(project_id, tool, mcp_id)
);

CREATE INDEX idx_mcp_project_assignments_tool_mcp
    ON mcp_project_assignments(tool, mcp_id);

CREATE TABLE skill_project_assignments (
    project_id TEXT NOT NULL REFERENCES projects(id) ON UPDATE CASCADE ON DELETE CASCADE,
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    skill_id TEXT NOT NULL REFERENCES skills(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(project_id, tool, skill_id)
);

CREATE INDEX idx_skill_project_assignments_tool_skill
    ON skill_project_assignments(tool, skill_id);

CREATE TRIGGER trg_mcp_project_assignment_reject_global
BEFORE INSERT ON mcp_project_assignments
WHEN EXISTS (
    SELECT 1 FROM mcp_global_assignments
    WHERE tool = NEW.tool AND mcp_id = NEW.mcp_id
)
BEGIN
    SELECT RAISE(ABORT, 'GLOBAL_ASSIGNMENT_INHERITED');
END;

CREATE TRIGGER trg_mcp_global_assignment_reject_project_duplicate
BEFORE INSERT ON mcp_global_assignments
WHEN EXISTS (
    SELECT 1 FROM mcp_project_assignments
    WHERE tool = NEW.tool AND mcp_id = NEW.mcp_id
)
BEGIN
    SELECT RAISE(ABORT, 'PROJECT_ASSIGNMENT_EXISTS');
END;

CREATE TRIGGER trg_skill_project_assignment_reject_global
BEFORE INSERT ON skill_project_assignments
WHEN EXISTS (
    SELECT 1 FROM skill_global_assignments
    WHERE tool = NEW.tool AND skill_id = NEW.skill_id
)
BEGIN
    SELECT RAISE(ABORT, 'GLOBAL_ASSIGNMENT_INHERITED');
END;

CREATE TRIGGER trg_skill_global_assignment_reject_project_duplicate
BEFORE INSERT ON skill_global_assignments
WHEN EXISTS (
    SELECT 1 FROM skill_project_assignments
    WHERE tool = NEW.tool AND skill_id = NEW.skill_id
)
BEGIN
    SELECT RAISE(ABORT, 'PROJECT_ASSIGNMENT_EXISTS');
END;

CREATE TRIGGER trg_mcp_project_assignment_update_reject_global
BEFORE UPDATE OF tool, mcp_id ON mcp_project_assignments
WHEN EXISTS (
    SELECT 1 FROM mcp_global_assignments
    WHERE tool = NEW.tool AND mcp_id = NEW.mcp_id
)
BEGIN
    SELECT RAISE(ABORT, 'GLOBAL_ASSIGNMENT_INHERITED');
END;

CREATE TRIGGER trg_mcp_global_assignment_update_reject_project_duplicate
BEFORE UPDATE OF tool, mcp_id ON mcp_global_assignments
WHEN EXISTS (
    SELECT 1 FROM mcp_project_assignments
    WHERE tool = NEW.tool AND mcp_id = NEW.mcp_id
)
BEGIN
    SELECT RAISE(ABORT, 'PROJECT_ASSIGNMENT_EXISTS');
END;

CREATE TRIGGER trg_skill_project_assignment_update_reject_global
BEFORE UPDATE OF tool, skill_id ON skill_project_assignments
WHEN EXISTS (
    SELECT 1 FROM skill_global_assignments
    WHERE tool = NEW.tool AND skill_id = NEW.skill_id
)
BEGIN
    SELECT RAISE(ABORT, 'GLOBAL_ASSIGNMENT_INHERITED');
END;

CREATE TRIGGER trg_skill_global_assignment_update_reject_project_duplicate
BEFORE UPDATE OF tool, skill_id ON skill_global_assignments
WHEN EXISTS (
    SELECT 1 FROM skill_project_assignments
    WHERE tool = NEW.tool AND skill_id = NEW.skill_id
)
BEGIN
    SELECT RAISE(ABORT, 'PROJECT_ASSIGNMENT_EXISTS');
END;

CREATE TABLE managed_targets (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    artifact_kind TEXT NOT NULL CHECK(artifact_kind IN ('provider', 'prompt', 'mcp', 'skill')),
    scope TEXT NOT NULL CHECK(scope IN ('global', 'project')),
    project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE CASCADE,
    target_path TEXT NOT NULL CHECK(
        target_path LIKE '/%' AND target_path != '/' AND instr(target_path, '//') = 0
        AND target_path NOT LIKE '%/../%' AND target_path NOT LIKE '%/./%'
        AND target_path NOT LIKE '%/..' AND target_path NOT LIKE '%/.'
        AND substr(target_path, -1) != '/'
    ),
    baseline_full_hash TEXT CHECK(
        baseline_full_hash IS NULL OR (
            length(baseline_full_hash) = 64 AND baseline_full_hash = lower(baseline_full_hash)
            AND baseline_full_hash NOT GLOB '*[^0-9a-f]*'
        )
    ),
    baseline_managed_hash TEXT CHECK(
        baseline_managed_hash IS NULL OR (
            length(baseline_managed_hash) = 64 AND baseline_managed_hash = lower(baseline_managed_hash)
            AND baseline_managed_hash NOT GLOB '*[^0-9a-f]*'
        )
    ),
    baseline_projection_json TEXT
        CHECK(baseline_projection_json IS NULL OR json_valid(baseline_projection_json)),
    last_status TEXT NOT NULL DEFAULT 'missing' CHECK(last_status IN (
        'in_sync', 'external_non_owned_change', 'external_owned_change', 'missing',
        'parse_error', 'permission_denied', 'policy_blocked', 'untrusted',
        'target_type_changed', 'failed'
    )),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1),
    CHECK(
        (baseline_full_hash IS NULL AND baseline_managed_hash IS NULL)
        OR
        (baseline_full_hash IS NOT NULL AND baseline_managed_hash IS NOT NULL)
    ),
    CHECK(
        (scope = 'global' AND project_id IS NULL)
        OR
        (scope = 'project' AND project_id IS NOT NULL AND artifact_kind IN ('mcp', 'skill'))
    )
);

CREATE UNIQUE INDEX uq_managed_targets_identity
    ON managed_targets(tool, artifact_kind, scope, ifnull(project_id, ''), target_path);
CREATE INDEX idx_managed_targets_project ON managed_targets(project_id);
CREATE INDEX idx_managed_targets_status ON managed_targets(last_status);

CREATE TABLE managed_items (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    target_id TEXT NOT NULL REFERENCES managed_targets(id) ON UPDATE CASCADE ON DELETE CASCADE,
    resource_kind TEXT NOT NULL CHECK(resource_kind IN ('provider', 'prompt', 'mcp', 'skill')),
    resource_id TEXT NOT NULL CHECK(
        length(resource_id) = 36 AND resource_id = lower(resource_id)
        AND substr(resource_id, 9, 1) = '-' AND substr(resource_id, 14, 1) = '-'
        AND substr(resource_id, 19, 1) = '-' AND substr(resource_id, 24, 1) = '-'
        AND length(replace(resource_id, '-', '')) = 32
        AND replace(resource_id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    external_key TEXT NOT NULL CHECK(length(external_key) > 0),
    last_applied_item_hash TEXT NOT NULL CHECK(
        length(last_applied_item_hash) = 64 AND last_applied_item_hash = lower(last_applied_item_hash)
        AND last_applied_item_hash NOT GLOB '*[^0-9a-f]*'
    ),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1),
    UNIQUE(target_id, external_key)
);

CREATE INDEX idx_managed_items_resource ON managed_items(resource_kind, resource_id);

CREATE TRIGGER trg_managed_items_kind_matches_target
BEFORE INSERT ON managed_items
WHEN NOT EXISTS (
    SELECT 1 FROM managed_targets
    WHERE id = NEW.target_id AND artifact_kind = NEW.resource_kind
)
BEGIN
    SELECT RAISE(ABORT, 'MANAGED_ITEM_KIND_MISMATCH');
END;

CREATE TRIGGER trg_managed_items_update_kind_matches_target
BEFORE UPDATE OF target_id, resource_kind ON managed_items
WHEN NOT EXISTS (
    SELECT 1 FROM managed_targets
    WHERE id = NEW.target_id AND artifact_kind = NEW.resource_kind
)
BEGIN
    SELECT RAISE(ABORT, 'MANAGED_ITEM_KIND_MISMATCH');
END;

CREATE TABLE sync_runs (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    kind TEXT NOT NULL CHECK(kind IN ('preview', 'apply', 'restore')),
    status TEXT NOT NULL CHECK(status IN (
        'previewed', 'applying', 'restoring', 'succeeded', 'failed', 'stale',
        'rolled_back', 'rollback_failed'
    )),
    scope TEXT NOT NULL CHECK(scope IN ('global', 'project')),
    project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    db_version INTEGER NOT NULL CHECK(db_version >= 0),
    journal_path TEXT CHECK(
        journal_path IS NULL OR (
            journal_path LIKE '/%' AND journal_path != '/' AND instr(journal_path, '//') = 0
            AND journal_path NOT LIKE '%/../%' AND journal_path NOT LIKE '%/./%'
            AND journal_path NOT LIKE '%/..' AND journal_path NOT LIKE '%/.'
            AND substr(journal_path, -1) != '/'
        )
    ),
    started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    finished_at TEXT,
    error_code TEXT CHECK(error_code IS NULL OR error_code IN (
        'NOT_FOUND', 'INVALID_INPUT', 'PARSE_ERROR', 'PERMISSION_DENIED',
        'POLICY_BLOCKED', 'UNTRUSTED_PROJECT', 'CONFLICT', 'STALE_PREVIEW',
        'PREVIEW_ALREADY_CONSUMED', 'WRITE_IN_PROGRESS', 'ATOMIC_WRITE_FAILED',
        'ROLLBACK_FAILED', 'SECRET_REDACTED', 'DATABASE_ERROR', 'MIGRATION_FAILED',
        'PERMISSION_AUDIT_FAILED'
    )),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1),
    CHECK(
        (scope = 'global' AND project_id IS NULL)
        OR
        (scope = 'project' AND project_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX uq_sync_runs_single_active_writer
    ON sync_runs((1)) WHERE status IN ('applying', 'restoring');
CREATE INDEX idx_sync_runs_project_started ON sync_runs(project_id, started_at DESC);
CREATE INDEX idx_sync_runs_status ON sync_runs(status);

CREATE TABLE sync_items (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    run_id TEXT NOT NULL REFERENCES sync_runs(id) ON UPDATE CASCADE ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES managed_targets(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    change_kind TEXT NOT NULL CHECK(change_kind IN (
        'add', 'update', 'delete', 'unchanged', 'warning', 'conflict'
    )),
    status TEXT NOT NULL CHECK(status IN (
        'in_sync', 'external_non_owned_change', 'external_owned_change', 'missing',
        'parse_error', 'permission_denied', 'policy_blocked', 'untrusted',
        'target_type_changed', 'failed'
    )),
    redacted_diff_json TEXT NOT NULL DEFAULT '{}'
        CHECK(json_valid(redacted_diff_json)),
    warning_codes_json TEXT NOT NULL DEFAULT '[]'
        CHECK(json_valid(warning_codes_json) AND json_type(warning_codes_json) = 'array'),
    error_code TEXT CHECK(error_code IS NULL OR error_code IN (
        'NOT_FOUND', 'INVALID_INPUT', 'PARSE_ERROR', 'PERMISSION_DENIED',
        'POLICY_BLOCKED', 'UNTRUSTED_PROJECT', 'CONFLICT', 'STALE_PREVIEW',
        'PREVIEW_ALREADY_CONSUMED', 'WRITE_IN_PROGRESS', 'ATOMIC_WRITE_FAILED',
        'ROLLBACK_FAILED', 'SECRET_REDACTED', 'DATABASE_ERROR', 'MIGRATION_FAILED',
        'PERMISSION_AUDIT_FAILED'
    )),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1),
    UNIQUE(run_id, target_id)
);

CREATE INDEX idx_sync_items_target ON sync_items(target_id);
CREATE INDEX idx_sync_items_status ON sync_items(status);

CREATE TRIGGER trg_sync_items_scope_matches_run
BEFORE INSERT ON sync_items
WHEN NOT EXISTS (
    SELECT 1
    FROM sync_runs AS run
    JOIN managed_targets AS target ON target.id = NEW.target_id
    WHERE run.id = NEW.run_id
      AND (
          (run.scope = 'global' AND target.scope = 'global')
          OR
          (run.scope = 'project' AND target.scope = 'project' AND run.project_id = target.project_id)
      )
)
BEGIN
    SELECT RAISE(ABORT, 'SYNC_ITEM_SCOPE_MISMATCH');
END;

CREATE TRIGGER trg_sync_items_update_scope_matches_run
BEFORE UPDATE OF run_id, target_id ON sync_items
WHEN NOT EXISTS (
    SELECT 1
    FROM sync_runs AS run
    JOIN managed_targets AS target ON target.id = NEW.target_id
    WHERE run.id = NEW.run_id
      AND (
          (run.scope = 'global' AND target.scope = 'global')
          OR
          (run.scope = 'project' AND target.scope = 'project' AND run.project_id = target.project_id)
      )
)
BEGIN
    SELECT RAISE(ABORT, 'SYNC_ITEM_SCOPE_MISMATCH');
END;

CREATE TABLE snapshots (
    id TEXT PRIMARY KEY CHECK(
        length(id) = 36 AND id = lower(id)
        AND substr(id, 9, 1) = '-' AND substr(id, 14, 1) = '-'
        AND substr(id, 19, 1) = '-' AND substr(id, 24, 1) = '-'
        AND length(replace(id, '-', '')) = 32
        AND replace(id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    run_id TEXT NOT NULL REFERENCES sync_runs(id) ON UPDATE CASCADE ON DELETE RESTRICT,
    target_path TEXT NOT NULL CHECK(
        target_path LIKE '/%' AND target_path != '/' AND instr(target_path, '//') = 0
        AND target_path NOT LIKE '%/../%' AND target_path NOT LIKE '%/./%'
        AND target_path NOT LIKE '%/..' AND target_path NOT LIKE '%/.'
        AND substr(target_path, -1) != '/'
    ),
    snapshot_path TEXT NOT NULL UNIQUE CHECK(
        snapshot_path LIKE '/%' AND snapshot_path != '/' AND instr(snapshot_path, '//') = 0
        AND snapshot_path NOT LIKE '%/../%' AND snapshot_path NOT LIKE '%/./%'
        AND snapshot_path NOT LIKE '%/..' AND snapshot_path NOT LIKE '%/.'
        AND substr(snapshot_path, -1) != '/'
    ),
    content_hash TEXT CHECK(
        content_hash IS NULL OR (
            length(content_hash) = 64 AND content_hash = lower(content_hash)
            AND content_hash NOT GLOB '*[^0-9a-f]*'
        )
    ),
    file_mode INTEGER CHECK(file_mode IS NULL OR file_mode BETWEEN 0 AND 4095),
    target_type TEXT NOT NULL CHECK(target_type IN ('file', 'directory', 'symlink', 'missing')),
    link_target TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    row_version INTEGER NOT NULL DEFAULT 1 CHECK(row_version >= 1),
    CHECK(
        (target_type = 'symlink' AND link_target IS NOT NULL)
        OR
        (target_type != 'symlink' AND link_target IS NULL)
    )
);

CREATE INDEX idx_snapshots_run ON snapshots(run_id);
CREATE INDEX idx_snapshots_target ON snapshots(target_path, created_at DESC);

-- 主表的 row_version 不能回退；调用方未显式递增时由数据库自动递增。
CREATE TRIGGER trg_provider_profiles_row_version_guard
BEFORE UPDATE ON provider_profiles WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_provider_profiles_row_version_bump
AFTER UPDATE ON provider_profiles WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE provider_profiles
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_prompt_profiles_row_version_guard
BEFORE UPDATE ON prompt_profiles WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_prompt_profiles_row_version_bump
AFTER UPDATE ON prompt_profiles WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE prompt_profiles
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_mcp_servers_row_version_guard
BEFORE UPDATE ON mcp_servers WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_mcp_servers_row_version_bump
AFTER UPDATE ON mcp_servers WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE mcp_servers
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_skills_row_version_guard
BEFORE UPDATE ON skills WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_skills_row_version_bump
AFTER UPDATE ON skills WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE skills
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_projects_row_version_guard
BEFORE UPDATE ON projects WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_projects_row_version_bump
AFTER UPDATE ON projects WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE projects
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_managed_targets_row_version_guard
BEFORE UPDATE ON managed_targets WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_managed_targets_row_version_bump
AFTER UPDATE ON managed_targets WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE managed_targets
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_managed_items_row_version_guard
BEFORE UPDATE ON managed_items WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_managed_items_row_version_bump
AFTER UPDATE ON managed_items WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE managed_items
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_sync_runs_row_version_guard
BEFORE UPDATE ON sync_runs WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_sync_runs_row_version_bump
AFTER UPDATE ON sync_runs WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE sync_runs
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_sync_items_row_version_guard
BEFORE UPDATE ON sync_items WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_sync_items_row_version_bump
AFTER UPDATE ON sync_items WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE sync_items
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;

CREATE TRIGGER trg_snapshots_row_version_guard
BEFORE UPDATE ON snapshots WHEN NEW.row_version < OLD.row_version
BEGIN SELECT RAISE(ABORT, 'ROW_VERSION_MUST_INCREASE'); END;
CREATE TRIGGER trg_snapshots_row_version_bump
AFTER UPDATE ON snapshots WHEN NEW.row_version = OLD.row_version
BEGIN
    UPDATE snapshots
    SET row_version = max(NEW.row_version, OLD.row_version + 1),
        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    WHERE id = NEW.id;
END;
