-- Soft delete for action items.
--
-- Deleting an item must stay recoverable: the production database has no
-- point-in-time recovery and only a nightly dump, so a destructive delete
-- between dumps would lose up to a day of history irrecoverably. Marking the
-- row keeps the item, its notes and its full status history intact, and keeps
-- the item's ID permanently claimed so it can never be reissued.
--
-- NULL deleted_at means live. Every read path filters on it.
ALTER TABLE action_items ADD COLUMN deleted_at TIMESTAMPTZ;
ALTER TABLE action_items ADD COLUMN deleted_by_id INTEGER REFERENCES users(id);

-- Every list query now carries `deleted_at IS NULL`, and live items are the
-- overwhelming majority, so a partial index keeps those lookups on the same
-- vendor-scoped path they used before.
CREATE INDEX idx_action_items_vendor_live
    ON action_items(vendor_id)
    WHERE deleted_at IS NULL;
