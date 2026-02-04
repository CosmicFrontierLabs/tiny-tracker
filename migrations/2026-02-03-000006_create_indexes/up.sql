CREATE INDEX idx_action_items_vendor ON action_items(vendor_id);
CREATE INDEX idx_action_items_owner ON action_items(owner_id);
CREATE INDEX idx_action_items_created_by ON action_items(created_by_id);
CREATE INDEX idx_action_items_due_date ON action_items(due_date);
CREATE INDEX idx_notes_action_item ON notes(action_item_id);
CREATE INDEX idx_notes_author ON notes(author_id);
CREATE INDEX idx_status_history_item ON status_history(action_item_id);
CREATE INDEX idx_status_history_changed_at ON status_history(action_item_id, changed_at DESC);
