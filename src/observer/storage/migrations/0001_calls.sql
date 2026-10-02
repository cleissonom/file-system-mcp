PRAGMA application_id = 1296257103;

CREATE TABLE completed_calls (
    id INTEGER PRIMARY KEY,
    sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
    started_at_ms INTEGER NOT NULL CHECK (started_at_ms >= 0),
    completed_at_ms INTEGER NOT NULL CHECK (completed_at_ms >= 0),
    tool TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('success', 'tool_error', 'protocol_error')),
    duration_ms REAL NOT NULL CHECK (duration_ms >= 0 AND duration_ms <= 1.7976931348623157e308),
    request_bytes INTEGER NOT NULL CHECK (request_bytes >= 0),
    response_bytes INTEGER NOT NULL CHECK (response_bytes >= 0)
) STRICT;
