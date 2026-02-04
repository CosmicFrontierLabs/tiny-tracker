CREATE TABLE categories (
    id SERIAL PRIMARY KEY,
    vendor_id INTEGER NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(vendor_id, name)
);

-- Add default categories for existing vendors
INSERT INTO categories (vendor_id, name) 
SELECT v.id, c.name 
FROM vendors v 
CROSS JOIN (VALUES 
    ('Programmatic'),
    ('SW / Ops'),
    ('Mechanical'),
    ('ADCS'),
    ('Systems'),
    ('ConOps')
) AS c(name);

-- Add category_id column to action_items
ALTER TABLE action_items ADD COLUMN category_id INTEGER REFERENCES categories(id);

-- Migrate existing category strings to category_id
UPDATE action_items ai
SET category_id = c.id
FROM categories c
WHERE c.vendor_id = ai.vendor_id AND c.name = ai.category;

-- Make category_id required and drop old column
ALTER TABLE action_items ALTER COLUMN category_id SET NOT NULL;
ALTER TABLE action_items DROP COLUMN category;
