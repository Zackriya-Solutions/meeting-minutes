'use client';
// VALUEOS: dependency-injection boundary for the flow. Screens call useValueOs() and never
// construct a client/service directly, so tests inject mock services and Phase 3 injects the
// real (plugin-backed) services — with no screen changes. Default = mock services.
import React, { createContext, useContext, useMemo } from 'react';
import type { ValueOsClient } from '../api/client';
import { MockValueOsClient, MockSeed, defaultMockSeed } from '../api/mockClient';
import { AuthService, createMockAuthService } from '../auth/authService';
import { InMemoryTokenStore } from '../auth/tokenStore';
import { ConfigService, createMockConfigService } from '../config/configService';
import { DigestGenerator, MockDigestGenerator } from '../digest/digest';
import { InMemoryPendingUploadStore, PendingUploadQueue } from '../upload/pendingQueue';

export interface ValueOsServices {
  client: ValueOsClient;
  auth: AuthService;
  config: ConfigService;
  digest: DigestGenerator;
  uploadQueue: PendingUploadQueue;
}

/** ⚠️ MOCK services — wires all mock impls together. The running app uses these until the
 *  Phase 3 real transport (tauri-plugin-oauth + stronghold + real HTTP) is approved/wired. */
export function createMockServices(opts?: { seed?: MockSeed }): ValueOsServices {
  const client = new MockValueOsClient(opts?.seed ?? defaultMockSeed());
  const auth = createMockAuthService(client, new InMemoryTokenStore());
  const config = createMockConfigService();
  const digest = new MockDigestGenerator();
  const uploadQueue = new PendingUploadQueue(client, new InMemoryPendingUploadStore());
  return { client, auth, config, digest, uploadQueue };
}

const Ctx = createContext<ValueOsServices | null>(null);

export function ValueOsProvider({
  services,
  children,
}: {
  services?: ValueOsServices;
  children: React.ReactNode;
}) {
  const value = useMemo(() => services ?? createMockServices(), [services]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useValueOs(): ValueOsServices {
  const v = useContext(Ctx);
  if (!v) throw new Error('useValueOs must be used within a ValueOsProvider');
  return v;
}
