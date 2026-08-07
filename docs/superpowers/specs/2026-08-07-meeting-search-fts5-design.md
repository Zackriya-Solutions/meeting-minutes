# Search across all meetings (SQLite FTS5)

**Date:** 2026-08-07
**Status:** approved, not yet implemented

## Problem

Search today only reaches raw transcript rows. Ask "when did we decide to drop
CoreML" and you get nothing, because that sentence lives in the *summary*, which
the index cannot see. The existing query also scans, cannot rank, and returns an
unbounded result set.

Current implementation, `database/repositories/transcript.rs:87-140`:

```sql
SELECT m.id, m.title, t.transcript, t.timestamp
FROM meetings m JOIN transcripts t ON m.id = t.meeting_id
WHERE LOWER(t.transcript) LIKE ?          -- '%query%'
```

Five concrete defects:

1. **Scope.** Only `transcripts.transcript`. Summaries (`summary_processes.result`),
   notes (`meeting_notes.notes_markdown`), and titles are invisible.
2. **Leading wildcard** defeats any index — full scan of every transcript row.
3. **No ranking.** Rows come back in arbitrary storage order.
4. **No limit.** Every match crosses the IPC boundary, even though the UI keeps
   at most one per meeting (`Sidebar/index.tsx:197`, `searchResults.find`).
5. **Substring, not word-aware.** `drop` matches `eavesdropping`; `coreml metal`
   matches nothing unless the two words happen to be adjacent.

## Approach

One FTS5 virtual table covering all four content kinds, kept current by SQL
triggers, queried by a rewritten `search_transcripts`. FTS5 fixes 2, 3 and 5
outright and supplies `snippet()` and `bm25()`, so the hand-rolled snippet
helper goes away.

**No new dependency.** `sqlx`'s `sqlite` feature forces `sqlx-sqlite/bundled`,
and `libsqlite3-sys`'s bundled build passes `-DSQLITE_ENABLE_FTS5`
unconditionally. FTS5 is already in the binary.

### Schema

New migration, `<timestamp>_add_search_index.sql`:

```sql
CREATE VIRTUAL TABLE search_index USING fts5(
  text,
  meeting_id UNINDEXED,
  kind UNINDEXED,
  ts UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 2'
);
```

`kind` is one of `transcript`, `summary`, `notes`, `title`. One row per
transcript segment; one row each for a meeting's notes, summary, and title.

`ts` carries the source row's own timestamp — `transcripts.timestamp` for
transcript rows, `meetings.created_at` for the other three. Today's query
returns `t.timestamp`, the position within the meeting, and the UI displays it;
substituting `meetings.created_at` for every kind would quietly drop that.
Storing it in the index keeps the field truthful without a second join.

Text is duplicated into the index rather than using an external-content table.
External content would avoid the copy but needs a `content_rowid` join and three
sets of triggers against `TEXT PRIMARY KEY` tables. A year of meetings is a few
MB of prose; duplicating it is not worth that complexity.

### Extracting prose from the summary JSON

`summary_processes.result` is a serialized `serde_json::Value` with a shallow,
regular shape (`frontend/src/types/index.ts:39-53`):

```json
{ "SectionKey": { "title": "…", "blocks": [ { "id": "…", "type": "…", "content": "…", "color": "…" } ] } }
```

Indexing it raw would put block UUIDs and `color`/`type` values into the index,
so a search for "text" would match every summary ever generated. The prose comes
out in pure SQL via JSON1, which keeps index maintenance inside the triggers:

```sql
SELECT group_concat(value, ' ') FROM json_tree(new.result)
 WHERE key IN ('content', 'title') AND type = 'text'
```

`meeting_notes.notes_markdown` is already plain markdown and is indexed directly.

### Keeping the index current

Triggers, not Rust call sites, so a future write path cannot silently skip the
index:

| Table | Events |
|---|---|
| `transcripts` | insert, update of `transcript`, delete |
| `meeting_notes` | insert, update of `notes_markdown`, delete |
| `summary_processes` | insert, update of `result` |
| `meetings` | update of `title`; delete → purge every row for that `meeting_id` |

Update triggers delete the prior row for that source then re-insert, since FTS5
has no upsert.

The `meetings` delete trigger purges by `meeting_id` explicitly rather than
relying on `ON DELETE CASCADE` to fire the child tables' delete triggers —
cascade only fires triggers when `recursive_triggers` is on, which is not
something to depend on.

The same migration backfills existing rows with four `INSERT … SELECT`
statements.

### Query

`TranscriptsRepository::search_transcripts` keeps its signature and, apart from
one added field, its return type:

```sql
WITH ranked AS (
  SELECT s.meeting_id, s.kind, s.ts, bm25(search_index) AS rank,
         snippet(search_index, 0, '<mark>', '</mark>', '…', 12) AS ctx,
         ROW_NUMBER() OVER (PARTITION BY s.meeting_id
                            ORDER BY bm25(search_index)) AS rn
  FROM search_index s
  WHERE search_index MATCH ?
)
SELECT r.meeting_id, m.title, r.ctx, r.ts, r.kind
FROM ranked r JOIN meetings m ON m.id = r.meeting_id
WHERE r.rn = 1
ORDER BY r.rank
LIMIT 50;
```

Best-ranked row per meeting via `ROW_NUMBER()`, not the `GROUP BY` bare-column
trick — that shortcut only picks the matching row under `min()`/`max()`, and the
ranking expression here is `bm25()`. Window functions need SQLite 3.25+; the
bundled build is far newer.

`LIMIT 50` matches how the UI consumes results (one hit per meeting, filtering a
sidebar list).

### Sanitizing the query string

This is the one trust boundary and the one place not to be terse. Raw user text
in `MATCH` is a syntax error on `"`, `*`, `-`, `:` and `(`, so an unsanitized
query makes search fail on ordinary typing.

```rust
/// `coreml drop` -> `"coreml" "drop"*`
fn to_fts_query(raw: &str) -> String
```

Quote every whitespace-separated token, doubling any `"` inside it; join with a
space for implicit AND; suffix `*` on the final token for prefix matching as the
user types. Quoting renders every special character literal, so there is no
character class to get wrong. An empty result means the caller returns an empty
vec, as it does today.

### Result type and UI

`TranscriptSearchResult` (`api/api.rs:41-47`) gains one field:

```rust
pub kind: String,   // "transcript" | "summary" | "notes" | "title"
```

`Sidebar/index.tsx` renders it as a small label on each hit. Without it a
summary hit is indistinguishable from a transcript hit, which makes the widened
scope invisible to the user. This is the only frontend change.

### Deletions

`TranscriptsRepository::get_match_context` (`transcript.rs:123-140`) is removed;
`snippet()` replaces it.

## Testing

One unit test on `to_fts_query`: multi-word input, embedded quotes, punctuation,
empty and whitespace-only input.

One integration test against an in-memory SQLite pool: run the migration, insert
a transcript row and a summary row for two meetings, assert a two-word query
matches, that a summary-only term is found, and that results carry the right
`kind` and collapse to one row per meeting.

No test framework, no fixtures.

## Explicitly out of scope

- **Per-kind bm25 weighting.** Add when transcript noise visibly outranks summary
  hits in practice.
- **Stemming** (`porter` tokenizer). Meeting search is mostly proper nouns and
  jargon, where stemming hurts more than it helps.
- **Embeddings / semantic search.** FTS5 first; revisit only if lexical search
  provably misses what users look for.
- **`LIKE` fallback.** FTS5 is compiled in unconditionally, so there is nothing
  to fall back from.
