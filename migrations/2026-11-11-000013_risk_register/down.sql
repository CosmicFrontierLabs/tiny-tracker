-- Reverting discards every risk and every item-to-risk link.
DROP TABLE action_item_risks;
DROP TABLE risks;
DROP SEQUENCE risk_number_seq;
