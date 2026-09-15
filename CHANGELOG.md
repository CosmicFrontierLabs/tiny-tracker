# Changelog

Notable changes to Tiny Tracker. Newest first.

The project has no release tags; entries are grouped under `Unreleased` until
one exists. Anything requiring a deployment change — a new environment variable,
a new secret — belongs here, because the deploy config lives in a different repo
(`cf-services`) and this file is the handoff.

## Unreleased

### Added

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

- Migration `2026-09-15-000011_soft_delete_action_items` adds `deleted_at` and
  `deleted_by_id` to `action_items`, plus a partial index on live rows. Both
  columns are nullable with no default, so the migration is additive and safe to
  run against existing data — every existing item is live.
