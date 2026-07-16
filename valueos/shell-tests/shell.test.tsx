import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ValueOsShell } from '@/valueos/shell/ValueOsShell';
import { MockValueOsClient, defaultMockSeed } from '@/valueos/api/mockClient';
import { createMockAuthService } from '@/valueos/auth/authService';
import { InMemoryTokenStore } from '@/valueos/auth/tokenStore';
import { createMockConfigService } from '@/valueos/config/configService';
import { MockDigestGenerator } from '@/valueos/digest/digest';
import { PendingUploadQueue, InMemoryPendingUploadStore } from '@/valueos/upload/pendingQueue';
import type { ValueOsServices } from '@/valueos/context/ValueOsProvider';
import type { EntitlementState } from '@/valueos/api/types';

// Reused model-download screen auto-completes (models already present) so the flow
// advances download → config in tests.
vi.mock('@/contexts/OnboardingContext', () => ({
  useOnboarding: () => ({
    parakeetDownloaded: true,
    summaryModelDownloaded: true,
    recommendedSummaryModel: 'rec-model',
    startBackgroundDownloads: vi.fn(() => Promise.resolve()),
    retryParakeetDownload: vi.fn(() => Promise.resolve()),
  }),
}));

// Recording is upstream/native — mock our adapter so tests need no Tauri.
const rec = vi.hoisted(() => ({
  start: vi.fn(() => Promise.resolve()),
  stop: vi.fn(() => Promise.resolve('We discussed pricing and agreed next steps.')),
}));
vi.mock('@/valueos/capture/useRecordingController', () => ({
  useRecordingController: () => ({ isRecording: false, status: 'idle', transcriptText: '', start: rec.start, stop: rec.stop }),
}));

function makeServices(entitlement: EntitlementState = 'active') {
  const seed = defaultMockSeed();
  seed.entitlements['tenant-acme'] = entitlement;
  const client = new MockValueOsClient(seed);
  const services: ValueOsServices = {
    client,
    auth: createMockAuthService(client, new InMemoryTokenStore()),
    config: createMockConfigService({ pickResult: '/tmp/tx', writable: true }),
    digest: new MockDigestGenerator(),
    uploadQueue: new PendingUploadQueue(client, new InMemoryPendingUploadStore()),
  };
  return { services, client };
}

async function loginToConfig() {
  fireEvent.click(screen.getByTestId('valueos-proceed')); // landing → login
  fireEvent.click(screen.getByTestId('valueos-login-start')); // login → (download auto) → config
  await screen.findByTestId('valueos-config');
}

async function configToCapture() {
  fireEvent.click(screen.getByTestId('valueos-config-pick'));
  await screen.findByTestId('valueos-config-folder');
  fireEvent.click(screen.getByTestId('valueos-config-continue'));
  await screen.findByTestId('valueos-capture');
}

async function selectLeadTarget() {
  fireEvent.change(screen.getByTestId('valueos-capture-tenant'), { target: { value: 'tenant-acme' } });
  fireEvent.click(screen.getByTestId('valueos-capture-type-lead'));
  await screen.findByTestId('valueos-capture-target-lead-1');
  fireEvent.click(screen.getByTestId('valueos-capture-target-lead-1'));
}

beforeEach(() => {
  rec.start.mockClear();
  rec.stop.mockClear();
});

describe('ValueOS full flow', () => {
  it('landing shows VA branding and advances to login', () => {
    const { services } = makeServices();
    render(<ValueOsShell services={services} />);
    expect(screen.getByText('ValueOS Agent')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: /value accelerator/i })).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    expect(screen.getByTestId('valueos-login')).toBeInTheDocument();
  });

  it('entitled login proceeds through download to configuration', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    expect(screen.getByTestId('valueos-config')).toBeInTheDocument();
  });

  it.each(['expired', 'never'] as EntitlementState[])(
    'blocks (%s) with a subscription CTA and no path to capture',
    async (state) => {
      const { services } = makeServices(state);
      render(<ValueOsShell services={services} />);
      fireEvent.click(screen.getByTestId('valueos-proceed'));
      fireEvent.click(screen.getByTestId('valueos-login-start'));
      await screen.findByTestId('valueos-blocked');
      expect(screen.getByTestId('valueos-blocked-url')).toHaveTextContent('value-accelerator.io');
      expect(screen.queryByTestId('valueos-config')).toBeNull();
      expect(screen.queryByTestId('valueos-capture')).toBeNull();
    },
  );

  it('recording CANNOT start until tenant + type + target are all selected', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    await configToCapture();
    const startBtn = () => screen.getByTestId('valueos-capture-start') as HTMLButtonElement;
    expect(startBtn().disabled).toBe(true); // nothing selected
    fireEvent.change(screen.getByTestId('valueos-capture-tenant'), { target: { value: 'tenant-acme' } });
    expect(startBtn().disabled).toBe(true); // tenant only
    fireEvent.click(screen.getByTestId('valueos-capture-type-lead'));
    await screen.findByTestId('valueos-capture-target-lead-1');
    expect(startBtn().disabled).toBe(true); // tenant + type, no target
    fireEvent.click(screen.getByTestId('valueos-capture-target-lead-1'));
    expect(startBtn().disabled).toBe(false); // all three → enabled
  });

  it('records, then stores + digests + uploads BOTH artifacts to the selected target', async () => {
    const { services, client } = makeServices('active');
    const uploadSpy = vi.spyOn(client, 'uploadTranscript');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    await configToCapture();
    await selectLeadTarget();

    fireEvent.click(screen.getByTestId('valueos-capture-start'));
    expect(rec.start).toHaveBeenCalledTimes(1);
    await screen.findByTestId('valueos-capture-recording');
    fireEvent.click(screen.getByTestId('valueos-capture-stop'));

    await screen.findByTestId('valueos-finalize');
    await waitFor(() => expect(screen.getByTestId('valueos-finalize-status')).toHaveTextContent(/attached to/i));

    expect(uploadSpy).toHaveBeenCalledTimes(1);
    const [tenantId, activityType, targetId, req] = uploadSpy.mock.calls[0];
    expect(tenantId).toBe('tenant-acme');
    expect(activityType).toBe('lead');
    expect(targetId).toBe('lead-1');
    expect(req.raw_content).toBe('We discussed pricing and agreed next steps.'); // transcript
    expect(req.digest.length).toBeGreaterThan(0); // digest — both artifacts uploaded
    expect(req.idempotency_key).toBeTruthy();
  });
});
