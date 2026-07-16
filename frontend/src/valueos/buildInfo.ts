// VALUEOS: build identity, so the *installed* app can always tell you which commit it
// was built from (kills "am I running a stale build?" ambiguity). The values are injected
// at build time by CI (.github/workflows/valueos-build.yml) as NEXT_PUBLIC_* env vars,
// which Next inlines into the static export — exactly like NEXT_PUBLIC_VALUEOS_REAL/_SHELL.
// Local builds that don't set them simply read as "local build". Nothing here touches
// upstream; it's all in our namespace.

export interface ValueOsBuildInfo {
  /** Short commit hash for a CI build, or 'local' when no build id was injected. */
  id: string;
  /** ISO-8601 build timestamp (UTC) if injected, else null. */
  time: string | null;
  /** True when no build id was injected (i.e. a local/dev build). */
  isLocal: boolean;
  /** Compact human label, e.g. "build a1b2c3d · 2026-07-16 14:22 UTC" or "local build". */
  label: string;
}

/** Format an ISO timestamp to "YYYY-MM-DD HH:MM UTC" via pure string slicing (no Date,
 *  no timezone surprises). Falls back to the raw trimmed value if the shape is unexpected. */
function formatBuildTime(iso: string): string {
  const t = iso.trim();
  // Only claim "UTC" for an explicit Zulu timestamp (which is exactly what CI emits:
  // `date -u …Z`). Anything with an offset or an odd shape is shown raw, never mislabeled.
  if (/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?(\.\d+)?Z$/.test(t)) {
    return `${t.slice(0, 10)} ${t.slice(11, 16)} UTC`;
  }
  return t;
}

/** Pure resolver — takes an env-like bag so it's trivially unit-testable. */
export function resolveBuildInfo(env: {
  NEXT_PUBLIC_VALUEOS_BUILD?: string;
  NEXT_PUBLIC_VALUEOS_BUILD_TIME?: string;
}): ValueOsBuildInfo {
  const raw = (env.NEXT_PUBLIC_VALUEOS_BUILD ?? '').trim();
  const isLocal = raw === '';
  const id = isLocal ? 'local' : raw;
  // A time only means something alongside a build id; with no id it's dead data → null.
  const rawTime = (env.NEXT_PUBLIC_VALUEOS_BUILD_TIME ?? '').trim();
  const time = isLocal || rawTime === '' ? null : rawTime;

  const label = isLocal
    ? 'local build'
    : `build ${id}${time ? ` · ${formatBuildTime(time)}` : ''}`;

  return { id, time, isLocal, label };
}

// The literal `process.env.NEXT_PUBLIC_*` references below are what Next statically
// replaces at build time (it only substitutes literal member accesses, not a passed-in
// object) — so these two lines MUST reference the keys verbatim.
export const BUILD_INFO: ValueOsBuildInfo = resolveBuildInfo({
  NEXT_PUBLIC_VALUEOS_BUILD: process.env.NEXT_PUBLIC_VALUEOS_BUILD,
  NEXT_PUBLIC_VALUEOS_BUILD_TIME: process.env.NEXT_PUBLIC_VALUEOS_BUILD_TIME,
});
