-- Reverting discards every risk note and the whole risk change history.
DROP TABLE risk_history;
DROP TABLE risk_notes;
