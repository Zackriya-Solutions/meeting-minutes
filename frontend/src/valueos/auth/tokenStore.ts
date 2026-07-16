// VALUEOS: token persistence interface. Phase 3 the real implementation is backed by
// tauri-plugin-stronghold (encrypted vault). Until then, screens use the in-memory mock.
// Tokens carry ONLY the four agent scopes.
export interface TokenSet {
  accessToken: string;
  refreshToken?: string;
  /** epoch ms when the access token expires (access = 60 min per contract). */
  expiresAt: number;
  scopes: string[];
}

export interface TokenStore {
  save(tokens: TokenSet): Promise<void>;
  load(): Promise<TokenSet | null>;
  clear(): Promise<void>;
}

/** Access token considered expired 30s early to avoid edge races. */
export function isExpired(tokens: TokenSet, now = fixedNow()): boolean {
  return tokens.expiresAt - 30_000 <= now;
}

// NOTE: Date.now is fine at runtime; injectable for deterministic tests.
function fixedNow(): number {
  return Date.now();
}

/** ⚠️ MOCK store (in-memory, NOT secure, NOT persisted). Real store = stronghold (Phase 3). */
export class InMemoryTokenStore implements TokenStore {
  private tokens: TokenSet | null = null;
  async save(tokens: TokenSet): Promise<void> {
    this.tokens = tokens;
  }
  async load(): Promise<TokenSet | null> {
    return this.tokens;
  }
  async clear(): Promise<void> {
    this.tokens = null;
  }
}
