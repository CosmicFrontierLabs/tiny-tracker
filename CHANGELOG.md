# Changelog

Notable changes to Tiny Tracker. Newest first.

The project has no release tags; entries are grouped under `Unreleased` until
one exists. Anything requiring a deployment change — a new environment variable,
a new secret — belongs here, because the deploy config lives in a different repo
(`cf-services`) and this file is the handoff.

## Unreleased

### Added

- **`JWT_SECRET_FILE`.** The JWT secret can now be read from a file (e.g. a
  read-only bind mount at `/run/secrets/session.key`) instead of the
  `JWT_SECRET` env var, keeping it out of `docker inspect` and out of the
  environment Watchtower copies. Exactly one trailing `\n` or `\r\n` is
  stripped; the rest is used byte-for-byte, so a file holding the current
  `JWT_SECRET` value keeps every staff session and outstanding vendor magic
  link valid. Setting both variables, or pointing at a missing, unreadable,
  empty or non-UTF-8 file, fails startup in every mode. The image has no
  `USER`, so the container runs as root and can read the file at any mode.

  **Deployment change (cf-services):** to switch, write the current value to
  the host file, mount it, set `JWT_SECRET_FILE`, and **remove** `JWT_SECRET`
  in the same change — leaving both set stops the container. Rotating the
  secret invalidates all staff sessions and all outstanding vendor magic links.

- **Risk register.** A second register alongside action items, reachable from
  the header. Each risk records the module or area it is against, a category,
  the risk statement, cause, effect, severity and likelihood (0–5, half points
  allowed), mitigation, test type, verification test IDs, status (`Open`,
  `In work`, `Mitigated`, `Accepted`), trend (`New`, `Improving`, `Stable`,
  `Worsening`, `Closed`) and the date it was opened. Score (severity ×
  likelihood) and level (High ≥ 15, Medium ≥ 8, else Low) are computed, never
  stored. Risk IDs are one programme-wide series, `RISK-034`; action items
  keep their vendor prefix.

  Action items map into risks many-to-many: the item modal gains a "Risks"
  section to link and unlink, and the risk modal lists the items addressing
  it, with a multi-select picker to link several at once. A risk can also be
  created already linked to a set of items. Risks are staff-only and never
  appear in the vendor portal. Deleting a risk is admin-only and a soft delete, restorable with
  `cargo run -p cli -- restore-risk --id RISK-034`.

  An existing register can be loaded from a CSV export of the mastersheet:

  ```bash
  cargo run -p cli -- import-risks-csv --file register.csv --vendor NEU --dry-run
  ```

  No new environment variables.

- **Risk notes and change history.** Each risk now has the same two
  timelines as an action item: free-text notes, and a record of who changed
  its status, trend, severity or likelihood, when, from what to what, and
  why (an optional reason on the edit form). The risk modal shows them merged
  as an activity feed, and the home page sidebar includes risk events. The
  importer opens each risk's history on its Opened date and accepts an
  optional `Notes` column as the first note.

- **Vendor prefix rename.** The prefix is editable from Manage Vendors. A
  rename rewrites every action item ID for that vendor (`AD-001` becomes
  `NEW-001`) together with its notes, status history and risk links, in one
  transaction. Old deep links to the previous IDs stop resolving.

- **Admin-only item deletion, as a soft delete.** The ticket modal shows a
  `Delete` button to admins, which arms on the first click and only sends the
  request on the second. `DELETE /api/items/:id` marks the item deleted rather
  than removing it: the row stays, along with its notes and its full status
  history, and records who deleted it and when. From every other angle the item
  is gone — staff list, item view, deep links, activity feed, vendor portal, and
  vendor item counts all exclude it, and notes and status changes against it
  return 404.

  Soft rather than destructive because production has no point-in-time recovery
  and only a nightly dump: a destructive delete between dumps would lose up to a
  day of history irrecoverably. It also means the record of *what* was deleted
  is a database row that survives the container being recreated.

  Restoring is an admin CLI operation, deliberately not exposed in the web UI:

  ```bash
  cargo run -p cli -- list-deleted
  cargo run -p cli -- restore-item --id AD-001
  ```

  The owning vendor's `next_number` is untouched, so a deleted item's ID is
  never reissued.

- **`ADMIN_EMAILS` environment variable.** Comma-separated, case-insensitive
  list of the users allowed to perform destructive admin actions. Empty or
  unset admits nobody, which is the behaviour for any deployment that does not
  set it. Dev mode (`DEV_MODE=true`) grants admin unconditionally, since it
  already bypasses authentication.

  The list is consulted on every request rather than baked into the JWT, so
  adding or removing someone takes effect at their next request (after a
  restart picks up the new value) instead of whenever their 24h token expires.

  **Deployment action:** set this in `cf-services` for the tracker container.
  Initial value:

  ```
  ADMIN_EMAILS=matt@cosmicfrontier.org
  ```

  Without it, no one can delete items — the feature is simply inert, so this is
  a safe variable to add after the image ships rather than before.

### Database

- Migration `2026-11-12-000015_risk_activity` adds `risk_notes` and
  `risk_history`, and backfills a `created` history row for every existing
  risk. Additive.

- Migration `2026-11-12-000014_vendor_prefix_rename` re-declares the three
  foreign keys onto `action_items` with `ON UPDATE CASCADE`. No data changes.

- Migration `2026-11-11-000013_risk_register` adds `risks`,
  `action_item_risks` and the `risk_number_seq` sequence. Purely additive;
  existing data is untouched.

- Migration `2026-09-15-000011_soft_delete_action_items` adds `deleted_at` and
  `deleted_by_id` to `action_items`, plus a partial index on live rows. Both
  columns are nullable with no default, so the migration is additive and safe to
  run against existing data — every existing item is live.
