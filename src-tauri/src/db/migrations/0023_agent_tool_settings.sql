-- Agents 工具特有设置覆盖层。
-- 交集字段仍保留在 agents；每个首期工具按 (agent_id, tool) 保存一份经服务层
-- 白名单校验并规范化的 JSON 对象。后续工具扩展必须通过新迁移放宽 tool CHECK。
CREATE TABLE agent_tool_settings (
    agent_id TEXT NOT NULL REFERENCES agents(id) ON UPDATE CASCADE ON DELETE CASCADE,
    tool TEXT NOT NULL CHECK(tool IN ('claude', 'codex')),
    settings_json TEXT NOT NULL
        CHECK(json_valid(settings_json) AND json_type(settings_json) = 'object'
              AND length(settings_json) <= 16384),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(agent_id, tool)
);

CREATE INDEX idx_agent_tool_settings_tool_agent
    ON agent_tool_settings(tool, agent_id);
