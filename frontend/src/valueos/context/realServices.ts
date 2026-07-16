// VALUEOS: wires the REAL (native-backed) services. Selected by ValueOsProvider when
// NEXT_PUBLIC_VALUEOS_REAL=on. Default is OFF (mock) until the Phase-3 Rust module +
// minimal upstream edits (Cargo.toml + lib.rs) ship and are verified by the build.
import type { ValueOsServices } from './ValueOsProvider';
import { TauriValueOsClient } from '../api/tauriClient';
import { createTauriAuthService } from '../auth/tauriAuthService';
import { createTauriConfigService } from '../config/tauriConfigService';
import { createTauriDigest } from '../digest/tauriDigest';
import { PendingUploadQueue } from '../upload/pendingQueue';
import { LocalStoragePendingUploadStore } from '../upload/localStorageQueueStore';

export const valueosRealTransportEnabled: boolean =
  (process.env.NEXT_PUBLIC_VALUEOS_REAL ?? 'off') === 'on';

export function createRealServices(): ValueOsServices {
  const client = new TauriValueOsClient();
  return {
    client,
    auth: createTauriAuthService(client),
    config: createTauriConfigService(),
    digest: createTauriDigest(),
    uploadQueue: new PendingUploadQueue(client, new LocalStoragePendingUploadStore()),
  };
}
