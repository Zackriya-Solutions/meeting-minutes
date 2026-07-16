// VALUEOS: single indirection over Tauri's invoke, so the real transport is trivially
// mockable in tests (mock '@/valueos/transport/tauri') and the @tauri-apps import lives in
// exactly one place.
export { invoke } from '@tauri-apps/api/core';
