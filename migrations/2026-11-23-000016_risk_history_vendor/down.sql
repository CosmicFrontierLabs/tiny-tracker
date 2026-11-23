-- Reverting discards the record of vendor moves; the risks keep their
-- current vendor.
DELETE FROM risk_history WHERE field = 'vendor';

ALTER TABLE risk_history
    DROP CONSTRAINT risk_history_field_check,
    ADD CONSTRAINT risk_history_field_check
        CHECK (field IN ('created', 'status', 'trend', 'severity', 'likelihood'));
