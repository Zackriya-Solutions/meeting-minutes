// VALUEOS: REAL auth — login/logout/session live in the Rust module (browser PKCE via
// tauri-plugin-oauth loopback; tokens in the OS keychain). The webview only triggers them
// and never handles the token. Entitlement summary reuses the real client.
import type { ValueOsClient } from '../api/client';
import { AuthService, summarizeEntitlements } from './authService';
import { callValueOs } from '../transport/invoke';

export function createTauriAuthService(client: ValueOsClient): AuthService {
  return {
    isLoggedIn: () => callValueOs<boolean>('valueos_is_logged_in'),
    login: () => callValueOs<void>('valueos_login'),
    logout: () => callValueOs<void>('valueos_logout'),
    loadEntitlementSummary: () => summarizeEntitlements(client),
  };
}
