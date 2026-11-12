-- Restores the original constraints. Data is untouched; a prefix rename
-- performed while the cascade was in place stays renamed.
ALTER TABLE notes
    DROP CONSTRAINT notes_action_item_id_fkey,
    ADD CONSTRAINT notes_action_item_id_fkey
        FOREIGN KEY (action_item_id) REFERENCES action_items(id);

ALTER TABLE status_history
    DROP CONSTRAINT status_history_action_item_id_fkey,
    ADD CONSTRAINT status_history_action_item_id_fkey
        FOREIGN KEY (action_item_id) REFERENCES action_items(id);

ALTER TABLE action_item_risks
    DROP CONSTRAINT action_item_risks_action_item_id_fkey,
    ADD CONSTRAINT action_item_risks_action_item_id_fkey
        FOREIGN KEY (action_item_id) REFERENCES action_items(id)
        ON DELETE CASCADE;
