-- Let a vendor's prefix be renamed without orphaning anything.
--
-- An action item's user-facing ID ("AD-001") is also its primary key, and
-- notes, status history and risk links all point at that string. Renaming a
-- vendor's prefix therefore rewrites those IDs, and the child tables must
-- follow. ON UPDATE CASCADE makes the database do that in the same
-- statement, so a rename is one transaction and cannot leave a note pointing
-- at an ID that no longer exists. Delete behaviour is unchanged.
ALTER TABLE notes
    DROP CONSTRAINT notes_action_item_id_fkey,
    ADD CONSTRAINT notes_action_item_id_fkey
        FOREIGN KEY (action_item_id) REFERENCES action_items(id)
        ON UPDATE CASCADE;

ALTER TABLE status_history
    DROP CONSTRAINT status_history_action_item_id_fkey,
    ADD CONSTRAINT status_history_action_item_id_fkey
        FOREIGN KEY (action_item_id) REFERENCES action_items(id)
        ON UPDATE CASCADE;

ALTER TABLE action_item_risks
    DROP CONSTRAINT action_item_risks_action_item_id_fkey,
    ADD CONSTRAINT action_item_risks_action_item_id_fkey
        FOREIGN KEY (action_item_id) REFERENCES action_items(id)
        ON UPDATE CASCADE ON DELETE CASCADE;
