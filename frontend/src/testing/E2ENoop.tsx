// Production stand-in for E2EBootstrap: next.config.js swaps this module in
// via NormalModuleReplacementPlugin whenever NEXT_PUBLIC_E2E_TESTING !== '1',
// keeping the Tauri browser mocks (and @tauri-apps/api/mocks) out of the
// production bundle entirely.
export function E2EBootstrap() {
  return null;
}
