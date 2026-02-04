CREATE TABLE status_history (
    id SERIAL PRIMARY KEY,
    action_item_id VARCHAR(20) NOT NULL REFERENCES action_items(id),
    status VARCHAR(50) NOT NULL,
    changed_by_id INTEGER NOT NULL REFERENCES users(id),
    changed_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    comment TEXT
);
