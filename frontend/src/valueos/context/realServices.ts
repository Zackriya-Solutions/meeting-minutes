// VALUEOS: wires the REAL (native-backed) services. This is the DEFAULT — packaged builds
// set NEXT_PUBLIC_VALUEOS_REAL=on (CI) so Next inlines it to a literal `true`. Mock is
// opt-in ONLY: set NEXT_PUBLIC_VALUEOS_REAL=off for browser/dev, or inject services in tests.
// The packaged desktop app therefore always uses the native transport (real login, native
// folder picker, real tenants/leads) — never the Acme/Ada-Lovelace mock seed.
import type { ValueOsServices } from './ValueOsProvider';
import { TauriValueOsClient } from '../api/tauriClient';
import { createTauriAuthService } from '../auth/tauriAuthService';
import { createTauriConfigService } from '../config/tauriConfigService';
import { createTauriDigest } from '../digest/tauriDigest';
import { PendingUploadQueue } from '../upload/pendingQueue';
import { LocalStoragePendingUploadStore } from '../upload/localStorageQueueStore';
import { LocalStorageTranscriptHistory } from '../history/transcriptHistory';
import { createUpdater } from '../updater/updater';
import { realUpdaterNative, localStorageUpdaterStore } from '../updater/nativeUpdater';
import { ApiBugReportService } from '../bugreport/service';

// Real transport is now the DEFAULT (config is baked in the native module). Set
// NEXT_PUBLIC_VALUEOS_REAL=off for mock/dev (tests inject services directly and are
// unaffected).
export const valueosRealTransportEnabled: boolean =
  (process.env.NEXT_PUBLIC_VALUEOS_REAL ?? 'on') !== 'off';

export function createRealServices(): ValueOsServices {
  const client = new TauriValueOsClient();
  return {
    client,
    auth: createTauriAuthService(client),
    config: createTauriConfigService(),
    digest: createTauriDigest(),
    uploadQueue: new PendingUploadQueue(client, new LocalStoragePendingUploadStore()),
    history: new LocalStorageTranscriptHistory(),
    updater: createUpdater({ client, native: realUpdaterNative, store: localStorageUpdaterStore }),
    bugReport: new ApiBugReportService(),
  };
}
