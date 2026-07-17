// VALUEOS: diagnostic helper — fetches the CLAIMS (not the token/secret) of the stored
// access token, so a 401 from the API can be triaged (is the token a valid agent access
// token, or is the backend misconfigured?). Backed by the native valueos_debug_token_claims
// command, which decodes only the JWT payload.
import { callValueOs } from '../transport/invoke';

export interface TokenClaims {
  client_id?: string | null;
  token_use?: string | null;
  scope?: string | null;
  iss?: string | null;
  exp?: number | null;
  sub?: string | null;
  username?: string | null;
}

/** Returns the stored access token's claims, or null if not logged in. */
export async function getAccessTokenClaims(): Promise<TokenClaims | null> {
  const v = await callValueOs<TokenClaims | null>('valueos_debug_token_claims');
  return v ?? null;
}
