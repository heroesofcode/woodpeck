# Portable progress format (schema version 3)

The wire model lives in `src/sync/format.rs`. Git and SQLite are adapters around
these serde types; a future server can exchange the same snapshot and use the
pure join in `src/sync/merge.rs`.

Every file is UTF-8 pretty JSON, with recursively sorted object keys, two-space
indentation and one trailing newline. Maps use stable identities, never SQLite
row IDs. There are no absolute paths, remote URLs, credentials, or machine
configuration in the snapshot. Times are Unix seconds except `updated_at`
(Unix milliseconds) and `local_day` (a proleptic Gregorian day number, where
1970-01-01 is 719163). Clock synchronization between computers is recommended.

| File | Contents |
| --- | --- |
| `manifest.json` | `{ "app": "woodpeck", "schema_version": 3 }` |
| `settings.json` | Preference name to `{ "value": ..., "updated_at": ... }` |
| `sets/<uid>.json` | Set identity, timestamped metadata fields, creation time, seed, ordered full puzzle snapshots, and cycles keyed by UID |
| `attempts/<set-uid>.json` | Immutable attempts sorted by `(created_at, uid)` |
| `review.json` | Puzzle ID to review identity, originating set UIDs, latest addition time, and retained resolution time |

The only supported preferences are the fields in `Settings`: board and piece
appearance, the sound effects master switch, tactical labels, auto advance, color theme, user
rating, and shuffle preference. The master switch retains the boolean wire key
`completion_sound`; legacy `false` disables all effects. Volume defaults to 35%,
is stored locally as `sound_volume` (0–100), and is excluded from snapshots and
preference clocks. Imports preserve the local volume. This keeps schema 3
compatible with older clients and lets each device use its own listening level.
Window geometry, audio devices, library/import
state, data paths, and all sync configuration stay local. The puzzle library is
excluded; each set includes its own complete puzzle data (`id`, `fen`, `moves`,
`rating`, `rd`, `popularity`, `nb_plays`, `themes`). A second computer can train
imported sets with an empty library. XP and streaks are rebuilt from attempts
and completed cycles, rather than added together as counters.

## Set theme fields

Since schema 2, a set's metadata carries three theme fields instead of the
single schema 1 `focus_theme`:

| Field | Value |
| --- | --- |
| `include_themes` | Sorted, distinct Lichess theme keys; empty means any theme |
| `exclude_themes` | Sorted, distinct Lichess theme keys a puzzle must not have |
| `theme_match` | `"any"` (at least one included theme) or `"all"` (every included theme) |

Exclusions always win, and a key may not be in both lists. Like the other
set parameters, these fields are fixed when the set is created and merge field
by field with the rules below.

## Set rating range

Since schema 3, a set's difficulty is an explicit, inclusive rating range
instead of schema 1–2's `target_rating`, `variance` and
`restrict_recommended`:

| Field | Value |
| --- | --- |
| `rating_min` | Lowest puzzle rating in the set, inclusive, 400–3200 |
| `rating_max` | Highest puzzle rating in the set, inclusive, 400–3200, at least `rating_min + 100` |

A range is never widened during generation. If too few puzzles match, the set is
not created.

### Schema history and compatibility

| Version | Change |
| --- | --- |
| 1 | Initial format; sets had an optional `focus_theme` string |
| 2 | `focus_theme` became `include_themes`, `exclude_themes` and `theme_match` |
| 3 | `target_rating`, `variance` and `restrict_recommended` became `rating_min` and `rating_max` |

Readers upgrade older snapshots in memory before validating them. A schema 1
set with `focus_theme: "fork"` becomes `include_themes: ["fork"]`,
`exclude_themes: []`, `theme_match: "any"`; a null focus becomes an empty
include list. All three fields keep the legacy field's `updated_at` (or the
set's creation time if it had none), matching what the SQLite migration
records locally, so an upgraded remote and a migrated local copy join without
spurious changes.

A schema 1 or 2 set's `target_rating ± variance` becomes the range
`rating_min = clamp(target − variance, 400, 3100)` and
`rating_max = max(min(target + variance, 3000), rating_min + 100)`. This is the
band that older releases searched first. The extra rule only applies at the old
edges, where it widens a too-narrow band upward to the minimum width. Both range
fields take the later `updated_at` of `target_rating` and `variance` (or the
set's creation time). `restrict_recommended` is dropped because a range is
always explicit. The SQLite migration applies the same rule and clocks. A
schema 1 snapshot is upgraded through schema 2 first.

The next push writes schema 3. A current snapshot that still contains a legacy
field (`focus_theme`, `target_rating`, `variance`, `restrict_recommended`) is
rejected as an unknown field. Apps that only know older schemas stop syncing
when they see a newer one (see below) until upgraded.

## Identity and merge rules

- Sets and attempts have random 128-bit lowercase hex UIDs generated with
  SQLite `randomblob(16)`. Migration backfills existing records atomically.
- A cycle UID is `<set-uid>-<number>`. Two computers starting the same next
  cycle share this identity and the deterministic order. Earliest start time
  wins; latest completion time wins. A union of partial answers can complete a
  cycle. Every attempt remains in history; cycle statistics and resume position
  count each puzzle once, choosing the first attempt by `(created_at, uid)`.
- Set metadata fields and preferences join independently by maximum
  `(updated_at, canonical compact JSON value)`. The value comparison breaks
  equal-clock ties deterministically. Frozen puzzle membership, seed, creation
  time, and cycle order must agree for a shared UID; inconsistency stops sync.
- Attempt logs join by UID. Identical attempts deduplicate. A shared UID with
  different payloads is an error, never a reason to drop an attempt.
- Review entries join by puzzle ID. `set_uids` is a union; `uid` is the smallest
  originating review UID. `added_at` is the maximum miss time, including misses
  while already pending. `resolved_at` is the maximum retained correct-review
  time, or null. The resolved state is derived: `resolved_at > added_at`.
  Equal times remain queued. Correct cycle answers and unsuccessful review
  attempts never resolve an item. A later miss requeues it. SQLite keeps a
  membership row per originating set, with a unique composite UID; the wire
  entry groups them into one puzzle identity.

The join is commutative, associative and idempotent for valid canonical states.
Sets and cycles are unions: deleting a local set does not delete its remote
history, so a subsequent sync restores it. There is no distributed deletion
operation in schema 3. Changing this behavior requires a versioned tombstone
format. Unknown preference/set fields, malformed records, and inconsistent
identities stop sync. A newer `schema_version` always stops sync before import,
working-tree writes or push; automatic retries pause until a manual retry or
configuration change. Upgrade the app to understand a newer schema.

## Git adapter and local durability

`$DATA/sync/` is an independent Git repository. The URL, opt-in flag, last sync,
status, last error, and acknowledged local generation live only in the local
SQLite `sync_config` setting; origin also lives in this sync repository's
`.git/config`. The source checkout is never used as a data directory.

The adapter fetches branches and reads remote files with `git show`. It prefers
`main`, accepts `master` or a sole other branch, and uses `main` for a new empty
remote. It imports a semantic union inside a transaction that includes any
training recorded during the fetch, then takes a consistent snapshot and
local change generation. It builds a tree with `hash-object --no-filters` and
`update-index`, and creates a commit with the remote and local heads as parents
(without duplicate parents). It never checks out remote content, runs a Git
text merge, or force pushes. A rejected push triggers another fetch/join/push,
at most three times within the operation's deadline. Unrelated remote files,
such as an initial README, remain in the remote tree.

Commits always use the sync-only identity `woodpeck <woodpeck@localhost>` through
Git `-c` options. Global configuration is untouched. Commit/push signing and
hooks are disabled for this generated repository; credential helpers and SSH
configuration remain available. Embedded HTTP credentials, passwords and URL
query/fragment parameters are rejected. Child stdin is closed,
`GIT_TERMINAL_PROMPT=0` and noninteractive credential flags are set. Output is
drained concurrently, and raw stderr (which may contain sensitive URLs) is not
stored. Operations have a 45-second total deadline, including at most three
push attempts. A local lease serializes sync operations, expiring after two
minutes following a crash.

The worker has its own SQLite connection and channels; network work holds no
application service lock. It pulls on enabled startup and syncs after 10 seconds
without local changes, including session/cycle completion and settings changes.
SQLite change generations persist with the progress transaction, so a crash
cannot hide unsynced progress. Failures preserve pending state and retry with
5-second exponential backoff capped at 5 minutes, and again on next startup.
Shutdown requests one best-effort sync, waits outside the native event loop for
at most 3 seconds, then exits. Pending work survives if that budget expires.
Disabling prevents future automatic sync; an operation already running may
finish. Disabling does not delete local progress, the local Git history, or the
private remote repository.
