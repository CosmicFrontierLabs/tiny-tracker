-- Conversation and change history for risks.
--
-- A register is reviewed over months, so a risk needs the same two timelines
-- an action item has: free-text notes, and a record of who moved its status,
-- trend, severity or likelihood, when, and why. Without the second, "trend"
-- is only ever an assertion; with it, score movement can be checked against
-- the evidence.

CREATE TABLE risk_notes (
    id SERIAL PRIMARY KEY,
    risk_id VARCHAR(20) NOT NULL REFERENCES risks(id),
    note_date DATE NOT NULL DEFAULT CURRENT_DATE,
    author_id INTEGER NOT NULL REFERENCES users(id),
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_risk_notes_risk ON risk_notes(risk_id);

-- One row per field that changed. An edit that moves both severity and
-- likelihood writes two rows sharing a changed_at, which the UI groups.
-- 'created' marks the start of the timeline and has no old/new value.
CREATE TABLE risk_history (
    id SERIAL PRIMARY KEY,
    risk_id VARCHAR(20) NOT NULL REFERENCES risks(id),
    changed_by_id INTEGER NOT NULL REFERENCES users(id),
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    field VARCHAR(20) NOT NULL
        CHECK (field IN ('created', 'status', 'trend', 'severity', 'likelihood')),
    old_value TEXT,
    new_value TEXT,
    comment TEXT
);

CREATE INDEX idx_risk_history_risk ON risk_history(risk_id);

-- Every existing risk gets a 'created' row so no timeline starts blank.
INSERT INTO risk_history (risk_id, changed_by_id, changed_at, field, comment)
SELECT id, created_by_id, created_at, 'created', 'Risk created'
FROM risks;
