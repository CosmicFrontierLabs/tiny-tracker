-- Reverse: add back category column and drop category_id
ALTER TABLE action_items ADD COLUMN category VARCHAR(50);

UPDATE action_items ai
SET category = c.name
FROM categories c
WHERE c.id = ai.category_id;

ALTER TABLE action_items ALTER COLUMN category SET NOT NULL;
ALTER TABLE action_items DROP COLUMN category_id;

DROP TABLE categories;
