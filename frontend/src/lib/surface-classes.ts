export const SURFACE_BG: Record<number, string> = {
  1: "bg-[var(--elevation-2)]",
  2: "bg-[var(--elevation-2)]",
  3: "bg-[var(--elevation-2)]",
  4: "bg-[var(--elevation-2)]",
  5: "bg-[var(--elevation-2)]",
  6: "bg-[var(--elevation-2)]",
  7: "bg-[var(--elevation-2)]",
  8: "bg-[var(--elevation-2)]",
};

export const SURFACE_SHADOW: Record<number, string> = {
  1: "shadow-[var(--shadow-1)]",
  2: "shadow-[var(--shadow-2)]",
  3: "shadow-[var(--shadow-3)]",
  4: "shadow-[var(--shadow-4)]",
  5: "shadow-[var(--shadow-5)]",
  6: "shadow-[var(--shadow-6)]",
  7: "shadow-[var(--shadow-7)]",
  8: "shadow-[var(--shadow-8)]",
};

export function surfaceClasses(bgLevel: number, shadowLevel: number = bgLevel): string {
  // Round after clamping so a fractional level can't index out of the lookup
  // tables (which would render "undefined undefined").
  const bg = Math.round(Math.max(1, Math.min(8, bgLevel)));
  const shadow = Math.round(Math.max(1, Math.min(8, shadowLevel)));
  return `${SURFACE_BG[bg]} ${SURFACE_SHADOW[shadow]}`;
}
