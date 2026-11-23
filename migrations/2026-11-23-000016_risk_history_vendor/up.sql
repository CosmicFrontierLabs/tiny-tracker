-- A risk can be moved to a different vendor, and the move is part of its
-- history like any other tracked change. Risk IDs carry no vendor prefix,
-- so this is a plain column update; the history row records old and new
-- prefix so the timeline reads "vendor NEU -> AD".
ALTER TABLE risk_history
    DROP CONSTRAINT risk_history_field_check,
    ADD CONSTRAINT risk_history_field_check
        CHECK (field IN ('created', 'status', 'trend', 'severity', 'likelihood', 'vendor'));
