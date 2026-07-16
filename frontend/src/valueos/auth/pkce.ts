// VALUEOS: OAuth2 PKCE (S256) helpers + authorize-URL builder, computed in the webview
// via Web Crypto (no native crate needed). Only the code_challenge + state go into the
// browser authorize URL; the verifier is kept in memory and handed to the (Phase 3)
// Rust token-exchange. Pure functions — unit-tested.
import type { ValueOsConfig } from '../api/types';

function base64url(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

const UNRESERVED = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~';

/** RFC 7636 code_verifier: 43–128 chars from the unreserved set. */
export function generateCodeVerifier(length = 64): string {
  const n = Math.min(128, Math.max(43, length));
  const rnd = new Uint8Array(n);
  crypto.getRandomValues(rnd);
  let out = '';
  for (let i = 0; i < n; i++) out += UNRESERVED[rnd[i] % UNRESERVED.length];
  return out;
}

/** S256 challenge = base64url(sha256(verifier)). */
export async function computeCodeChallenge(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier));
  return base64url(new Uint8Array(digest));
}

/** Opaque CSRF/state value. */
export function generateState(): string {
  const rnd = new Uint8Array(24);
  crypto.getRandomValues(rnd);
  return base64url(rnd);
}

/** Loopback redirect URI for the given port (must be registered in Cognito). */
export function loopbackRedirectUri(port: number): string {
  return `http://127.0.0.1:${port}/callback`;
}

export interface AuthorizeUrlParts {
  codeChallenge: string;
  state: string;
  redirectUri: string;
}

/** Builds the Cognito hosted-UI authorize URL (Authorization Code + PKCE, public client). */
export function buildAuthorizeUrl(cfg: ValueOsConfig, parts: AuthorizeUrlParts): string {
  const u = new URL(`${cfg.hostedUiBase}/oauth2/authorize`);
  u.searchParams.set('response_type', 'code');
  u.searchParams.set('client_id', cfg.clientId);
  u.searchParams.set('redirect_uri', parts.redirectUri);
  u.searchParams.set('scope', cfg.scopes.join(' '));
  u.searchParams.set('code_challenge', parts.codeChallenge);
  u.searchParams.set('code_challenge_method', 'S256');
  u.searchParams.set('state', parts.state);
  return u.toString();
}

/** A fresh PKCE + state bundle for one login attempt. */
export interface PkceSession {
  verifier: string;
  challenge: string;
  state: string;
  redirectUri: string;
  authorizeUrl: string;
}

export async function createPkceSession(cfg: ValueOsConfig): Promise<PkceSession> {
  const verifier = generateCodeVerifier();
  const challenge = await computeCodeChallenge(verifier);
  const state = generateState();
  const redirectUri = loopbackRedirectUri(cfg.callbackPorts[0]);
  const authorizeUrl = buildAuthorizeUrl(cfg, { codeChallenge: challenge, state, redirectUri });
  return { verifier, challenge, state, redirectUri, authorizeUrl };
}
