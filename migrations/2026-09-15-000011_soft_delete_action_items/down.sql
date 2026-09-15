-- Dropping these columns discards the record of which items were deleted, and
-- reverting leaves previously hidden items visible again.
DROP INDEX IF EXISTS idx_action_items_vendor_live;
ALTER TABLE action_items DROP COLUMN deleted_by_id;
ALTER TABLE action_items DROP COLUMN deleted_at;
