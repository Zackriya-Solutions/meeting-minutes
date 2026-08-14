/**
 * Title utilities shared between the auto-naming hook and tests.
 *
 * Mirrors the Rust `should_auto_name` heuristic so the frontend can make a
 * local fallback decision without a round-trip to the backend.
 */

/**
 * Returns true when a meeting title looks like a default/auto-generated
 * placeholder rather than a user-assigned name.
 *
 * Heuristic (mirrors `auto_naming.rs`):
 *  - starts with "Meeting " or "meeting_"
 *  - is purely numeric/dash/space/colon/T (a timestamp)
 *  - is shorter than 5 characters
 */
export function isDefaultTitle(title: string | null | undefined): boolean {
  if (!title) return true;
  const t = title.trim();
  if (t.length < 5) return true;
  if (t.startsWith('Meeting ') || t.startsWith('meeting_')) return true;
  // Pure timestamp: only digits, dashes, spaces, colons, or 'T' allowed.
  return [...t].every(
    (c) => /\d/.test(c) || c === '-' || c === ' ' || c === ':' || c === 'T',
  );
}
