CREATE TABLE action_items (
    id VARCHAR(20) PRIMARY KEY,
    vendor_id INTEGER NOT NULL REFERENCES vendors(id),
    number INTEGER NOT NULL,
    title VARCHAR(500) NOT NULL,
    create_date DATE NOT NULL,
    created_by_id INTEGER NOT NULL REFERENCES users(id),
    due_date DATE,
    category VARCHAR(50) NOT NULL,
    owner_id INTEGER NOT NULL REFERENCES users(id),
    priority VARCHAR(20) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    UNIQUE(vendor_id, number)
);
