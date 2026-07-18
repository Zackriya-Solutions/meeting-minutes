// VALUEOS WS3: the real (native-backed) metadata source for a bug report. Resilient — a native
// failure degrades to "unknown" rather than blocking the report.
import { callValueOs } from '../transport/invoke';
import { BUILD_INFO } from '../buildInfo';
import type { MetadataSource } from './buildBugReport';

export const realMetadataSource: MetadataSource = {
  async appInfo() {
    try {
      const r = await callValueOs<{ platform: string; version: string }>('valueos_app_info');
      return r ?? { platform: 'unknown', version: BUILD_INFO.id };
    } catch {
      return { platform: 'unknown', version: BUILD_INFO.id };
    }
  },
  async installId() {
    try {
      return (await callValueOs<string>('valueos_install_id')) ?? 'unknown';
    } catch {
      return 'unknown';
    }
  },
  build: BUILD_INFO.label,
};
