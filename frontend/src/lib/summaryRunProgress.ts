/**
 * When to stop believing a summary run that still claims to be in progress.
 *
 * A generation lives in a spawned task, so it dies with the application. A row left in
 * `pending`/`processing` after that is never moved on by anyone: the meeting screen polls that
 * status forever (a spinner with nothing behind it) and the automatic backfill skips the meeting
 * as "already running". Startup recovery clears those rows, but this rule is what keeps a screen
 * that is already open — or a run that stalls mid-session — from spinning without end.
 *
 * Kept apart from the screen so the decision itself is testable: it is the part that a change to
 * the poller's own timeout, or to how `start_time` is recorded, would quietly break.
 */

/** Generation is bounded well below this — the frontend poller gives up at ~16.5 minutes. */
export const STALLED_SUMMARY_AFTER_MS = 20 * 60 * 1000;

export interface SummaryRunAge {
  /** `start_time` of the run as returned by `api_get_summary` (RFC 3339), if recorded. */
  startedAt?: string | null;
  /** When this screen first observed the run in progress — the fallback clock. */
  firstSeenAt: number;
  now: number;
}

/**
 * True when a run in progress has been going long enough that no worker can still be behind it.
 *
 * Rows written before `start_time` existed report no start; those are judged from the moment the
 * screen first saw them, so an unknown age cannot spin indefinitely either.
 */
export function isSummaryRunStalled({ startedAt, firstSeenAt, now }: SummaryRunAge): boolean {
  const startedAtMs = startedAt ? Date.parse(startedAt) : NaN;
  const runningSince = Number.isFinite(startedAtMs) ? startedAtMs : firstSeenAt;
  return now - runningSince > STALLED_SUMMARY_AFTER_MS;
}
