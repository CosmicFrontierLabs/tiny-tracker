-- Risk register.
--
-- A risk is a vendor-scoped record of something that might go wrong: what it
-- is, why, what it would cost, how likely and how bad, and what is being done
-- about it. Action items are the work; risks are the reasons for the work.
-- The two are linked many-to-many through action_item_risks, so one risk can
-- be retired by several items and one item can address several risks.
--
-- Score (severity x likelihood) and level (High / Medium / Low) are NOT
-- stored: both are pure functions of severity and likelihood, and storing
-- them would only create a second place for the truth to drift. The shared
-- crate owns that arithmetic so the frontend and backend cannot disagree.

-- Risk IDs are a single programme-wide series, "RISK-034", not per vendor.
-- Action items carry the vendor's prefix ("AD-001") because they are the
-- vendor's work; a risk is ours, about a vendor, and reads the same from
-- any register view. The number comes from a sequence so concurrent creates
-- cannot collide, and the importer bumps it past any numbers it preserves.
CREATE SEQUENCE risk_number_seq START 1;

CREATE TABLE risks (
    id VARCHAR(20) PRIMARY KEY,
    vendor_id INTEGER NOT NULL REFERENCES vendors(id),
    number INTEGER NOT NULL,
    -- The module, assembly or programme area the risk is against
    -- ("NSV-455", "All modules", "Program"). Free text with suggestions in
    -- the UI rather than a table: the set is small, vendor-specific and
    -- changes as hardware is renamed.
    component VARCHAR(100),
    category VARCHAR(100) NOT NULL,
    title VARCHAR(500) NOT NULL,
    cause TEXT,
    effect TEXT,
    -- Both on a 0-5 scale. Half points are in real use ("2.5", "3.5"), so
    -- these are not integers.
    severity DOUBLE PRECISION NOT NULL CHECK (severity >= 0 AND severity <= 5),
    likelihood DOUBLE PRECISION NOT NULL CHECK (likelihood >= 0 AND likelihood <= 5),
    mitigation TEXT,
    test_type VARCHAR(50),
    verification TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'Open'
        CHECK (status IN ('Open', 'In work', 'Mitigated', 'Accepted')),
    -- Trend is a human judgement recorded at review time, not derived from
    -- history. 'Closed' lives here rather than in status because that is how
    -- the register has always been kept: a mitigated risk stays 'Mitigated'
    -- and its trend records that it is no longer being watched.
    trend VARCHAR(20) NOT NULL DEFAULT 'New'
        CHECK (trend IN ('New', 'Improving', 'Stable', 'Worsening', 'Closed')),
    opened_on DATE NOT NULL DEFAULT CURRENT_DATE,
    created_by_id INTEGER NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Soft delete, same posture as action_items: nothing is destroyed, the
    -- ID stays claimed, and every read path filters on deleted_at IS NULL.
    deleted_at TIMESTAMPTZ,
    deleted_by_id INTEGER REFERENCES users(id),
    UNIQUE (number)
);

CREATE INDEX idx_risks_vendor_live ON risks(vendor_id) WHERE deleted_at IS NULL;

CREATE TABLE action_item_risks (
    action_item_id VARCHAR(20) NOT NULL REFERENCES action_items(id) ON DELETE CASCADE,
    risk_id VARCHAR(20) NOT NULL REFERENCES risks(id) ON DELETE CASCADE,
    linked_by_id INTEGER NOT NULL REFERENCES users(id),
    linked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (action_item_id, risk_id)
);

CREATE INDEX idx_action_item_risks_risk ON action_item_risks(risk_id);
