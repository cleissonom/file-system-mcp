ALTER TABLE completed_calls ADD COLUMN source TEXT NOT NULL DEFAULT 'unknown'
    CHECK (source IN ('unknown', 'chatgpt', 'chatgpt_work', 'codex', 'codex_cloud', 'openai_dot'));
ALTER TABLE completed_calls ADD COLUMN evidence TEXT NOT NULL DEFAULT 'unknown'
    CHECK (evidence IN ('unknown', 'client_reported', 'operator_configured'));
ALTER TABLE completed_calls ADD COLUMN transport TEXT NOT NULL DEFAULT 'unknown'
    CHECK (transport IN ('unknown', 'stdio', 'tunnel'));
ALTER TABLE completed_calls ADD COLUMN session_id INTEGER NOT NULL DEFAULT 0
    CHECK (session_id >= 0);
