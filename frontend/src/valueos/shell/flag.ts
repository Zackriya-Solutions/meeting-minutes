// VALUEOS: master switch for the branded shell entry point.
// When true, the app launches into the ValueOS Agent branded shell (see the seam in
// frontend/src/app/layout.tsx). Set NEXT_PUBLIC_VALUEOS_SHELL=off to fall back to the
// stock upstream onboarding/app (useful for debugging upstream behavior).
export const valueOsShellEnabled: boolean =
  (process.env.NEXT_PUBLIC_VALUEOS_SHELL ?? 'on') !== 'off';
