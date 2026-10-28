-- Optional cross-reference to the vendor's own ticketing system.
--
-- Free-form text rather than a structured table: an item usually maps to one
-- or two external tickets, and what staff have to hand is a pasted key
-- ("VND-4521") or a URL. The UI splits on commas and newlines for display
-- and turns anything that looks like a URL into a link; the server stores
-- the string as typed. NULL means no external reference.
ALTER TABLE action_items ADD COLUMN vendor_tracking_ids TEXT;
