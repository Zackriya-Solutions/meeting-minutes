import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ValueOsShell } from '@/valueos/shell/ValueOsShell';
import { MockValueOsClient, defaultMockSeed, type MockSeed } from '@/valueos/api/mockClient';
import { createMockAuthService } from '@/valueos/auth/authService';
import { InMemoryTokenStore } from '@/valueos/auth/tokenStore';
import { createMockConfigService } from '@/valueos/config/configService';
import { MockDigestGenerator } from '@/valueos/digest/digest';
import { PendingUploadQueue, InMemoryPendingUploadStore } from '@/valueos/upload/pendingQueue';
import { InMemoryTranscriptHistory } from '@/valueos/history/transcriptHistory';
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
  transcriptText: '',
}));
vi.mock('@/valueos/capture/useRecordingController', () => ({
  useRecordingController: () => ({
    isRecording: false,
    status: 'idle',
    transcriptText: rec.transcriptText,
    start: rec.start,
    stop: rec.stop,
  }),
}));

function servicesFromSeed(seed: MockSeed) {
  const client = new MockValueOsClient(seed);
  const services: ValueOsServices = {
    client,
    auth: createMockAuthService(client, new InMemoryTokenStore()),
    config: createMockConfigService({ pickResult: '/tmp/tx', writable: true }),
    digest: new MockDigestGenerator(),
    uploadQueue: new PendingUploadQueue(client, new InMemoryPendingUploadStore()),
    history: new InMemoryTranscriptHistory(),
  };
  return { services, client };
}

function makeServices(entitlement: EntitlementState = 'active') {
  const seed = defaultMockSeed();
  seed.entitlements['tenant-acme'] = entitlement;
  return servicesFromSeed(seed);
}

async function loginToConfig() {
  fireEvent.click(screen.getByTestId('valueos-proceed')); // landing → login
  fireEvent.click(screen.getByTestId('valueos-login-start')); // login → (download auto) → config
  await screen.findByTestId('valueos-config');
}

async function configToHome() {
  fireEvent.click(screen.getByTestId('valueos-config-pick'));
  await screen.findByTestId('valueos-config-folder');
  fireEvent.click(screen.getByTestId('valueos-config-continue'));
  await screen.findByTestId('valueos-home');
}

async function homeToCapture() {
  fireEvent.click(screen.getByTestId('valueos-home-new'));
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
  rec.transcriptText = '';
});

describe('ValueOS full flow', () => {
  it('landing shows VA branding + a build stamp, and advances to login', () => {
    const { services } = makeServices();
    render(<ValueOsShell services={services} />);
    expect(screen.getByText('ValueOS Agent')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: /value accelerator/i })).toBeInTheDocument();
    // The build stamp renders globally (every screen), so a stale build is always visible.
    expect(screen.getByTestId('valueos-build-stamp')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    expect(screen.getByTestId('valueos-login')).toBeInTheDocument();
    // …still visible on the login screen (where the hang happens).
    expect(screen.getByTestId('valueos-build-stamp')).toBeInTheDocument();
  });

  it('entitled login proceeds through download to configuration (first run — no folder yet)', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    expect(screen.getByTestId('valueos-config')).toBeInTheDocument();
  });

  it('skips the folder config on a later run when a folder is already saved', async () => {
    const seed = defaultMockSeed();
    const client = new MockValueOsClient(seed);
    const services: ValueOsServices = {
      client,
      auth: createMockAuthService(client, new InMemoryTokenStore()),
      config: createMockConfigService({ initialFolder: '/Users/me/ValueOS Transcripts', writable: true }),
      digest: new MockDigestGenerator(),
      uploadQueue: new PendingUploadQueue(client, new InMemoryPendingUploadStore()),
      history: new InMemoryTranscriptHistory(),
    };
    render(<ValueOsShell services={services} />);
    fireEvent.click(screen.getByTestId('valueos-proceed')); // landing → login
    fireEvent.click(screen.getByTestId('valueos-login-start')); // login → download(auto) → …
    await screen.findByTestId('valueos-home'); // straight to home — config was skipped
    expect(screen.queryByTestId('valueos-config')).toBeNull();
  });

  it.each(['expired', 'never'] as EntitlementState[])(
    'blocks (%s) with a CTA and no path to capture',
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

  it('gate uses /me/agent-tenants (no per-tenant entitlement enumeration)', async () => {
    const { services, client } = makeServices('active');
    const agentSpy = vi.spyOn(client, 'getAgentTenants');
    const entSpy = vi.spyOn(client, 'getEntitlement');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    expect(agentSpy).toHaveBeenCalledTimes(1); // single gate call
    expect(entSpy).not.toHaveBeenCalled(); // never enumerates /me/entitlements
  });

  it('blocks with a "no workspace" message when the user belongs to none', async () => {
    const { services } = servicesFromSeed({ tenants: [], entitlements: {}, leads: {}, opportunities: {} });
    render(<ValueOsShell services={services} />);
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    fireEvent.click(screen.getByTestId('valueos-login-start'));
    await screen.findByTestId('valueos-blocked');
    expect(screen.getByTestId('valueos-blocked')).toHaveTextContent(/don't belong to any ValueOS workspace/i);
  });

  it('blocks with a "no add-on" message when workspaces exist but lack the add-on', async () => {
    const { services } = servicesFromSeed({
      tenants: [{ id: 'tenant-acme', name: 'Acme', role: 'sales_user', roles: ['sales_user'] }],
      entitlements: { 'tenant-acme': 'expired' },
      leads: {},
      opportunities: {},
    });
    render(<ValueOsShell services={services} />);
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    fireEvent.click(screen.getByTestId('valueos-login-start'));
    await screen.findByTestId('valueos-blocked');
    expect(screen.getByTestId('valueos-blocked')).toHaveTextContent(
      /none of your workspaces have the ValueOS Agent add-on/i,
    );
  });

  it('recording CANNOT start until tenant + type + target are all selected', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    await configToHome();
    await homeToCapture();
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

  it('shows recognized speech live while recording', async () => {
    rec.transcriptText = 'Ada: I have questions on API limits and the timeline.';
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    await configToHome();
    await homeToCapture();
    await selectLeadTarget();
    fireEvent.click(screen.getByTestId('valueos-capture-start'));
    await screen.findByTestId('valueos-capture-recording');
    expect(screen.getByTestId('valueos-capture-live')).toHaveTextContent(
      'Ada: I have questions on API limits and the timeline.',
    );
  });

  it('records, then creates a call with BOTH artifacts via the composite /calls path', async () => {
    const { services, client } = makeServices('active');
    const callSpy = vi.spyOn(client, 'createCall');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    await configToHome();
    await homeToCapture();
    await selectLeadTarget();

    fireEvent.click(screen.getByTestId('valueos-capture-start'));
    expect(rec.start).toHaveBeenCalledTimes(1);
    await screen.findByTestId('valueos-capture-recording');
    fireEvent.click(screen.getByTestId('valueos-capture-stop'));

    await screen.findByTestId('valueos-finalize');
    await waitFor(() => expect(screen.getByTestId('valueos-finalize-status')).toHaveTextContent(/attached to/i));

    expect(callSpy).toHaveBeenCalledTimes(1);
    const [tenantId, req] = callSpy.mock.calls[0];
    expect(tenantId).toBe('tenant-acme');
    expect(req.name).toBe('Call with Ada Lovelace'); // user-chosen (auto-filled) call name
    expect(req.lead_id).toBe('lead-1'); // XOR link in the body
    expect(req.opportunity_id).toBeUndefined();
    expect(req.transcript.raw_content).toBe('We discussed pricing and agreed next steps.'); // transcript
    expect(req.transcript.digest.length).toBeGreaterThan(0); // agent-generated digest
    expect(req.idempotency_key).toBeTruthy();

    // Done → home → the captured transcript now appears in the local list.
    fireEvent.click(screen.getByTestId('valueos-finalize-done'));
    await screen.findByTestId('valueos-home');
    expect(screen.getByTestId('valueos-home-list')).toHaveTextContent('Ada Lovelace');
  });

  it('reuses the user-chosen call name when creating the call', async () => {
    const { services, client } = makeServices('active');
    const callSpy = vi.spyOn(client, 'createCall');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    await configToHome();
    await homeToCapture();
    await selectLeadTarget(); // auto-fills "Call with Ada Lovelace"…
    fireEvent.change(screen.getByTestId('valueos-capture-callname'), { target: { value: 'Q3 Renewal call' } });
    fireEvent.click(screen.getByTestId('valueos-capture-start'));
    await screen.findByTestId('valueos-capture-recording');
    fireEvent.click(screen.getByTestId('valueos-capture-stop'));
    await screen.findByTestId('valueos-finalize');
    await waitFor(() => expect(callSpy).toHaveBeenCalled());
    expect(callSpy.mock.calls[0][1].name).toBe('Q3 Renewal call'); // …but the edited name is used
  });

  it('re-gates mid-session when the selected workspace loses access during capture (§2.7)', async () => {
    const { services, client } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    await configToHome();
    await homeToCapture();
    // Admin disables the add-on after login; selecting the tenant triggers a lead search
    // that 403s (feat_agent) → the shell re-runs the gate → agent-tenants now empty → block.
    client.setEntitlement('tenant-acme', 'expired');
    fireEvent.change(screen.getByTestId('valueos-capture-tenant'), { target: { value: 'tenant-acme' } });
    fireEvent.click(screen.getByTestId('valueos-capture-type-lead'));
    await screen.findByTestId('valueos-blocked');
  });

  it('finalize handles a workspace that lost access during the meeting (re-gates)', async () => {
    const { services, client } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    await configToHome();
    await homeToCapture();
    await selectLeadTarget();
    fireEvent.click(screen.getByTestId('valueos-capture-start'));
    await screen.findByTestId('valueos-capture-recording');
    client.setEntitlement('tenant-acme', 'expired'); // lost during the meeting
    fireEvent.click(screen.getByTestId('valueos-capture-stop'));
    await screen.findByTestId('valueos-finalize');
    // upload 403 feat_agent → de-entitled state with a continue-to-re-gate control
    const cont = await screen.findByTestId('valueos-finalize-deentitled');
    fireEvent.click(cont);
    await screen.findByTestId('valueos-blocked'); // re-gate finds no entitled workspace
  });

  it('after config, home shows the transcript list + a New control', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToConfig();
    await configToHome();
    expect(screen.getByTestId('valueos-home-new')).toBeInTheDocument();
    expect(screen.getByTestId('valueos-home-empty')).toBeInTheDocument();
  });
});
