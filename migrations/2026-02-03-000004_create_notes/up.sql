CREATE TABLE notes (
    id SERIAL PRIMARY KEY,
    action_item_id VARCHAR(20) NOT NULL REFERENCES action_items(id),
    note_date DATE NOT NULL,
    author_id INTEGER NOT NULL REFERENCES users(id),
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW() NOT NULL
);
