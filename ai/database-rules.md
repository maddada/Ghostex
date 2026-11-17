# Database changes stay safe to go back from

Ghostex lets people go back to the version they had before an update (Settings > Updates > Go
back, and the message after a new version crashes twice in its first day; `apps/desktop/src/go_back/`).
Going back runs the **older** gxserver on the database the **newer** one already migrated. The
older gxserver skips migrations it doesn't know (`server/src/storage/setup.rs` only applies the
ones missing from `schema_migrations`), so going back works as long as the older version can still
read and write what the newer one left.

User decision (2026-11-17, Q3730): "for db changes we need to keep those backwards compatible so we
can rollback easily ... if we need to do a change and rolling back isn't possible for that specific
release then we can skip showing the easy rollback option in that case", checked automatically by
the release.

## The rules

1. **Only add.** New tables, new indexes, new columns. A new `NOT NULL` column needs a `DEFAULT`
   (SQLite requires it anyway), so rows the older version inserts still get a value.
2. **Never rename or delete in the same release that stops using something.** Stop reading and
   writing the old table or column first and ship that; delete it a few releases later, when nobody
   would go back that far. A table rebuild (create `x_next`, copy, drop `x`, rename) counts as a
   delete.
3. **No new unique index on an existing table.** The older version doesn't know the rule and its
   inserts would start failing. A unique index on a table created in the same release is fine.
4. **Triggers and views** need a review: one that only maintains something new (a revision stamp in
   `metadata`) is safe; mark the migration `-- go-back-safe: <why>` after checking that the older
   version's writes still succeed with it.
5. **Data changes** (`INSERT`, `UPDATE`, `DELETE`) are fine: the older version reads rows, not the
   migration.
6. **Settings files and other stored JSON follow the same rule:** add keys, never rename or change
   the meaning of an existing one in one step, and code that saves a settings file keeps keys it
   doesn't know.

Put every new migration's SQL in `server/src/storage/migrations/<id>.sql` and add it with
`include_str!` in `schema_migrations.rs`; the check can't read SQL written inline.

## The release check

`tooling/release-gpui/go-back-gate.mjs` reads the migrations added since the previous release tag
and fails the release on anything outside the rules above. It runs in `release:actions` before
dispatch, in `release-gpui.yml`'s `gates` job and in `release:preflight`. Run it by hand with
`node tooling/release-gpui/go-back-gate.mjs` (`--since vX.Y.Z` to compare with another release).

## When a release really can't go back

Set `noGoBackBelow` in `release-go-back.json` (repository root) to the release's own version and say
why:

```json
{ "noGoBackBelow": "12.0.0", "reason": "sessions moved to a new table" }
```

The check then passes, and that version (and every later one, until the field changes) offers no Go
back to anything older than `noGoBackBelow`: the message after crashes and Settings > Updates hide
it. Going back between later versions still works (12.1.0 back to 12.0.0). The desktop app reads
the file at build time (`apps/desktop/src/go_back/mod.rs`).
