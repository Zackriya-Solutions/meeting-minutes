import { describe, it, expect } from 'vitest';
import {
  generateCodeVerifier,
  computeCodeChallenge,
  buildAuthorizeUrl,
  loopbackRedirectUri,
  createPkceSession,
} from '@/valueos/auth/pkce';
import type { ValueOsConfig } from '@/valueos/api/types';

const cfg: ValueOsConfig = {
  region: 'eu-central-2',
  clientId: 'agent-client-id',
  hostedUiBase: 'https://example.auth.eu-central-2.amazoncognito.com',
  apiBase: 'https://example.com/api/agent/v1',
  scopes: ['valueos/read:tenants', 'valueos/read:leads', 'valueos/read:opportunities', 'valueos/write:transcripts'],
  callbackPorts: [8765, 14321],
};

describe('PKCE (S256)', () => {
  it('generates a verifier of valid length and charset', () => {
    const v = generateCodeVerifier();
    expect(v.length).toBeGreaterThanOrEqual(43);
    expect(v.length).toBeLessThanOrEqual(128);
    expect(v).toMatch(/^[A-Za-z0-9\-._~]+$/);
  });

  it('computes the S256 challenge per the RFC 7636 test vector', async () => {
    // RFC 7636 Appendix B
    const verifier = 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk';
    const challenge = await computeCodeChallenge(verifier);
    expect(challenge).toBe('E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM');
  });

  it('builds a correct authorize URL', () => {
    const url = new URL(
      buildAuthorizeUrl(cfg, { codeChallenge: 'chal', state: 'st', redirectUri: loopbackRedirectUri(8765) }),
    );
    expect(url.origin + url.pathname).toBe(`${cfg.hostedUiBase}/oauth2/authorize`);
    expect(url.searchParams.get('response_type')).toBe('code');
    expect(url.searchParams.get('client_id')).toBe('agent-client-id');
    expect(url.searchParams.get('redirect_uri')).toBe('http://127.0.0.1:8765/callback');
    expect(url.searchParams.get('code_challenge_method')).toBe('S256');
    expect(url.searchParams.get('code_challenge')).toBe('chal');
    expect(url.searchParams.get('state')).toBe('st');
    // ONLY the four agent scopes, space-joined
    expect(url.searchParams.get('scope')).toBe(cfg.scopes.join(' '));
  });

  it('createPkceSession yields a matching verifier→challenge and a loopback redirect', async () => {
    const s = await createPkceSession(cfg);
    expect(await computeCodeChallenge(s.verifier)).toBe(s.challenge);
    expect(s.redirectUri).toBe('http://127.0.0.1:8765/callback');
    expect(s.authorizeUrl).toContain('code_challenge=' + s.challenge);
  });
});
