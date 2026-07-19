import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { ValueOsShell } from '@/valueos/shell/ValueOsShell';
import { MockValueOsClient, defaultMockSeed, type MockSeed } from '@/valueos/api/mockClient';
import { createMockAuthService } from '@/valueos/auth/authService';
import { InMemoryTokenStore } from '@/valueos/auth/tokenStore';
import { createMockConfigService } from '@/valueos/config/configService';
import { MockDigestGenerator } from '@/valueos/digest/digest';
import { PendingUploadQueue, InMemoryPendingUploadStore } from '@/valueos/upload/pendingQueue';
import { InMemoryTranscriptHistory } from '@/valueos/history/transcriptHistory';
import { createUpdater } from '@/valueos/updater/updater';
import { MockBugReportService } from '@/valueos/bugreport/service';
import type { ValueOsServices } from '@/valueos/context/ValueOsProvider';
import type { EntitlementState } from '@/valueos/api/types';

// VALUEOS: end-to-end tests for the REDESIGNED flow (welcome → login+gate → setup → storage
// → dark-sidebar app with the New-transcript wizard, Recording screen, and the
// one-ongoing-transcript guard). Only OUR code is exercised; native/upstream is mocked.

// Reused model-download auto-completes (models already present) so setup advances instantly.
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
  confirmedText: '',
  partialText: '',
  lines: [] as { role: 'me' | 'other'; label: string; text: string; partial?: boolean }[],
  start: vi.fn(() => Promise.resolve()),
  stop: vi.fn(() => Promise.resolve('We discussed pricing and agreed next steps.')),
  pause: vi.fn(() => Promise.resolve()),
  resume: vi.fn(() => Promise.resolve()),
}));
vi.mock('@/valueos/capture/useRecordingController', () => ({
  useRecordingController: () => ({
    isRecording: false,
    status: 'idle',
    transcriptText: [rec.confirmedText, rec.partialText].filter(Boolean).join(' '),
    confirmedText: rec.confirmedText,
    partialText: rec.partialText,
    lines: rec.lines,
    wordCount: rec.confirmedText ? rec.confirmedText.trim().split(/\s+/).length : 0,
    start: rec.start,
    stop: rec.stop,
    pause: rec.pause,
    resume: rec.resume,
  }),
}));

function servicesFromSeed(seed: MockSeed) {
  const client = new MockValueOsClient(seed);
  const mem = new Map<string, string>();
  const services: ValueOsServices = {
    client,
    auth: createMockAuthService(client, new InMemoryTokenStore()),
    config: createMockConfigService({ pickResult: '/tmp/tx', writable: true }),
    digest: new MockDigestGenerator(),
    uploadQueue: new PendingUploadQueue(client, new InMemoryPendingUploadStore()),
    history: new InMemoryTranscriptHistory(),
    updater: createUpdater({
      client,
      native: {
        appInfo: async () => ({ platform: 'test', version: '0.0.0' }),
        installId: async () => 'test-install',
        download: async () => '/tmp/update',
        apply: async () => {},
      },
      store: { get: (k) => mem.get(k) ?? null, set: (k, v) => void mem.set(k, v) },
    }),
    bugReport: new MockBugReportService(),
  };
  return { services, client };
}

function makeServices(entitlement: EntitlementState = 'active') {
  const seed = defaultMockSeed();
  seed.entitlements['tenant-acme'] = entitlement;
  return servicesFromSeed(seed);
}

async function loginToStorage() {
  fireEvent.click(screen.getByTestId('valueos-proceed')); // welcome → login
  fireEvent.click(screen.getByTestId('valueos-login-start')); // login → (setup auto) → storage
  await screen.findByTestId('valueos-config');
}

async function storageToDashboard() {
  fireEvent.click(screen.getByTestId('valueos-config-pick'));
  await waitFor(() =>
    expect((screen.getByTestId('valueos-config-folder') as HTMLInputElement).value).toBe('/tmp/tx'),
  );
  fireEvent.click(screen.getByTestId('valueos-config-continue'));
  await screen.findByTestId('valueos-dashboard');
}

async function openWizardToRecord() {
  fireEvent.click(screen.getByTestId('valueos-new'));
  await screen.findByTestId('valueos-wizard');
  // step 1: tenant
  fireEvent.click(screen.getByTestId('valueos-wizard-tenant-tenant-acme'));
  fireEvent.click(screen.getByTestId('valueos-wizard-continue'));
  // step 2: type
  fireEvent.click(screen.getByTestId('valueos-wizard-type-lead'));
  fireEvent.click(screen.getByTestId('valueos-wizard-continue'));
  // step 3: record
  await screen.findByTestId('valueos-wizard-record-lead-1');
  fireEvent.click(screen.getByTestId('valueos-wizard-record-lead-1'));
  fireEvent.click(screen.getByTestId('valueos-wizard-continue'));
  // step 4: name
  await screen.findByTestId('valueos-wizard-name');
}

beforeEach(() => {
  rec.start.mockClear();
  rec.stop.mockClear();
  rec.pause.mockClear();
  rec.resume.mockClear();
  rec.confirmedText = '';
  rec.partialText = '';
  rec.lines = [];
});

describe('ValueOS redesigned flow', () => {
  it('welcome shows VA branding + a build stamp, and advances to login', () => {
    const { services } = makeServices();
    render(<ValueOsShell services={services} />);
    expect(screen.getByTestId('valueos-welcome')).toBeInTheDocument();
    expect(screen.getAllByRole('img', { name: /value accelerator/i }).length).toBeGreaterThan(0);
    expect(screen.getByTestId('valueos-build-stamp')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    expect(screen.getByTestId('valueos-login')).toBeInTheDocument();
    expect(screen.getByTestId('valueos-build-stamp')).toBeInTheDocument();
  });

  it('entitled login proceeds through setup to storage (first run — no folder yet)', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    expect(screen.getByTestId('valueos-config')).toBeInTheDocument();
  });

  it('skips the folder step on a later run when a folder is already saved', async () => {
    const { services: base, client } = makeServices('active');
    const services: ValueOsServices = {
      ...base,
      config: createMockConfigService({ initialFolder: '/Users/me/ValueOS Transcripts', writable: true }),
    };
    void client;
    render(<ValueOsShell services={services} />);
    fireEvent.click(screen.getByTestId('valueos-proceed'));
    fireEvent.click(screen.getByTestId('valueos-login-start'));
    await screen.findByTestId('valueos-dashboard'); // straight to dashboard — storage skipped
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
      expect(screen.queryByTestId('valueos-dashboard')).toBeNull();
    },
  );

  it('gate uses /me/agent-tenants (no per-tenant entitlement enumeration)', async () => {
    const { services, client } = makeServices('active');
    const agentSpy = vi.spyOn(client, 'getAgentTenants');
    const entSpy = vi.spyOn(client, 'getEntitlement');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    expect(agentSpy).toHaveBeenCalledTimes(1);
    expect(entSpy).not.toHaveBeenCalled();
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

  it('dashboard shows a New control + an empty state before any capture', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    expect(screen.getByTestId('valueos-new')).toBeInTheDocument();
    expect(screen.getByTestId('valueos-dashboard-empty')).toBeInTheDocument();
  });

  it('wizard: Continue is disabled until each step has a selection', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    fireEvent.click(screen.getByTestId('valueos-new'));
    await screen.findByTestId('valueos-wizard');
    const cont = () => screen.getByTestId('valueos-wizard-continue') as HTMLButtonElement;
    expect(cont().disabled).toBe(true); // no tenant
    fireEvent.click(screen.getByTestId('valueos-wizard-tenant-tenant-acme'));
    expect(cont().disabled).toBe(false);
    fireEvent.click(cont());
    expect(cont().disabled).toBe(true); // no type
    fireEvent.click(screen.getByTestId('valueos-wizard-type-lead'));
    expect(cont().disabled).toBe(false);
    fireEvent.click(cont());
    await screen.findByTestId('valueos-wizard-record-lead-1');
    expect(cont().disabled).toBe(true); // no record
    fireEvent.click(screen.getByTestId('valueos-wizard-record-lead-1'));
    expect(cont().disabled).toBe(false);
  });

  it('renders speaker-aligned chat lines (You vs Other) with a faded interim', async () => {
    // You (me) = blue/left; Other (someone else) = grey/right; interim = faded/italic.
    rec.lines = [
      { role: 'me', label: 'You', text: 'And the piece that would really move the needle', partial: false },
      { role: 'other', label: 'Other', text: 'earlier in the cycle, before the deal even reaches', partial: true },
    ];
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    await openWizardToRecord();
    fireEvent.click(screen.getByTestId('valueos-wizard-start'));
    await screen.findByTestId('valueos-recording');
    expect(rec.start).toHaveBeenCalledTimes(1);

    const me = screen.getByTestId('valueos-line-me');
    expect(me).toHaveTextContent('really move the needle');
    const other = screen.getByTestId('valueos-line-other');
    expect(other).toHaveTextContent('before the deal even reaches');
    expect(other).toHaveStyle({ fontStyle: 'italic' }); // interim
    expect(other).toHaveAttribute('data-partial', 'true');
    // An in-flight interim makes the activity indicator "recognizing".
    const activity = await screen.findByTestId('valueos-recording-activity');
    expect(activity).toHaveAttribute('data-activity', 'recognizing');
  });

  it('End & upload creates a call with BOTH artifacts via the composite /calls path', async () => {
    const { services, client } = makeServices('active');
    const callSpy = vi.spyOn(client, 'createCall');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    await openWizardToRecord();
    fireEvent.click(screen.getByTestId('valueos-wizard-start'));
    await screen.findByTestId('valueos-recording');
    fireEvent.click(screen.getByTestId('valueos-recording-end'));

    await screen.findByTestId('valueos-transcripts'); // returns to the list after upload
    await waitFor(() => expect(callSpy).toHaveBeenCalledTimes(1));
    const [tenantId, req] = callSpy.mock.calls[0];
    expect(tenantId).toBe('tenant-acme');
    expect(req.name).toBe('Discovery Call — Ada Lovelace'); // wizard default name
    expect(req.lead_id).toBe('lead-1');
    expect(req.opportunity_id).toBeUndefined();
    expect(req.transcript.raw_content).toBe('We discussed pricing and agreed next steps.');
    expect(req.transcript.digest.length).toBeGreaterThan(0);
    expect(req.idempotency_key).toBeTruthy();
    // the captured transcript now appears in the list
    expect(screen.getByTestId('valueos-transcripts')).toHaveTextContent('Ada Lovelace');
  });

  it('discards a transcript (with confirmation) — stops capture, no upload, no history entry', async () => {
    const { services, client } = makeServices('active');
    const callSpy = vi.spyOn(client, 'createCall');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    await openWizardToRecord();
    fireEvent.click(screen.getByTestId('valueos-wizard-start'));
    await screen.findByTestId('valueos-recording');

    // Discard is GUARDED: clicking it opens a confirmation and does nothing on its own.
    fireEvent.click(screen.getByTestId('valueos-recording-discard'));
    await screen.findByTestId('valueos-discard-confirm');
    // Backing out ("Keep recording") leaves the call running — still on the recording screen.
    fireEvent.click(screen.getByTestId('valueos-discard-cancel'));
    expect(screen.queryByTestId('valueos-discard-confirm')).toBeNull();
    expect(screen.getByTestId('valueos-recording')).toBeTruthy();

    // Confirming discards: capture stops, we return to the dashboard, nothing is uploaded.
    fireEvent.click(screen.getByTestId('valueos-recording-discard'));
    fireEvent.click(await screen.findByTestId('valueos-discard-confirm-btn'));
    await screen.findByTestId('valueos-dashboard');
    expect(rec.stop).toHaveBeenCalled();
    expect(callSpy).not.toHaveBeenCalled(); // never uploaded
    // and it left NO transcript in the list
    fireEvent.click(screen.getByTestId('valueos-nav-transcripts'));
    expect(screen.getByTestId('valueos-transcripts')).not.toHaveTextContent('Ada Lovelace');
  });

  it('reuses the user-chosen call name when creating the call', async () => {
    const { services, client } = makeServices('active');
    const callSpy = vi.spyOn(client, 'createCall');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    await openWizardToRecord();
    fireEvent.change(screen.getByTestId('valueos-wizard-name'), { target: { value: 'Q3 Renewal call' } });
    fireEvent.click(screen.getByTestId('valueos-wizard-start'));
    await screen.findByTestId('valueos-recording');
    fireEvent.click(screen.getByTestId('valueos-recording-end'));
    await waitFor(() => expect(callSpy).toHaveBeenCalled());
    expect(callSpy.mock.calls[0][1].name).toBe('Q3 Renewal call');
  });

  it('BLOCKS a second transcript while one is recording (one-ongoing constraint)', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    await openWizardToRecord();
    fireEvent.click(screen.getByTestId('valueos-wizard-start'));
    await screen.findByTestId('valueos-recording');
    // navigate away to the dashboard — the call keeps recording (on-air banner shows)
    fireEvent.click(screen.getByTestId('valueos-nav-dashboard'));
    await screen.findByTestId('valueos-onair-banner');
    // trying to start a new one is refused with an explicit error, and NO wizard opens
    fireEvent.click(screen.getByTestId('valueos-new'));
    expect(screen.getByTestId('valueos-guard-error')).toHaveTextContent(/already in progress/i);
    expect(screen.queryByTestId('valueos-wizard')).toBeNull();
  });

  it('re-gates mid-session when the selected workspace loses access during the wizard (§2.7)', async () => {
    const { services, client } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    fireEvent.click(screen.getByTestId('valueos-new'));
    await screen.findByTestId('valueos-wizard');
    fireEvent.click(screen.getByTestId('valueos-wizard-tenant-tenant-acme'));
    fireEvent.click(screen.getByTestId('valueos-wizard-continue'));
    fireEvent.click(screen.getByTestId('valueos-wizard-type-lead'));
    client.setEntitlement('tenant-acme', 'expired'); // admin disables the add-on
    fireEvent.click(screen.getByTestId('valueos-wizard-continue')); // record step lists leads → 403
    await screen.findByTestId('valueos-blocked');
  });

  it('handles a workspace that lost access during the meeting (re-gates on upload)', async () => {
    const { services, client } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    await openWizardToRecord();
    fireEvent.click(screen.getByTestId('valueos-wizard-start'));
    await screen.findByTestId('valueos-recording');
    client.setEntitlement('tenant-acme', 'expired'); // lost during the meeting
    fireEvent.click(screen.getByTestId('valueos-recording-end'));
    await screen.findByTestId('valueos-blocked'); // upload 403 → re-gate → no entitled workspace
  });

  it('opening a saved transcript shows the reader with the digest above the transcript', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    await openWizardToRecord();
    fireEvent.click(screen.getByTestId('valueos-wizard-start'));
    await screen.findByTestId('valueos-recording');
    fireEvent.click(screen.getByTestId('valueos-recording-end'));
    const reader = await screen.findByTestId('valueos-transcripts-reader');
    expect(within(reader).getByText(/Digest/i)).toBeInTheDocument();
    expect(reader).toHaveTextContent('We discussed pricing and agreed next steps.');
  });

  it('deletes a local transcript (list entry + stored file), keeping the ValueOS copy', async () => {
    const { services } = makeServices('active');
    const removeSpy = vi.spyOn(services.history, 'remove');
    const deleteFileSpy = vi.spyOn(services.config, 'deleteTranscriptFile');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    await openWizardToRecord();
    fireEvent.click(screen.getByTestId('valueos-wizard-start'));
    await screen.findByTestId('valueos-recording');
    fireEvent.click(screen.getByTestId('valueos-recording-end'));
    await screen.findByTestId('valueos-transcripts-reader');

    // delete → inline confirm → gone
    fireEvent.click(screen.getByTestId('valueos-transcript-delete'));
    fireEvent.click(screen.getByTestId('valueos-transcript-delete-confirm'));
    await screen.findByTestId('valueos-transcripts-placeholder'); // reader cleared

    expect(removeSpy).toHaveBeenCalledTimes(1);
    expect(deleteFileSpy).toHaveBeenCalledTimes(1); // stored .txt removed too
  });

  it('reports a bug from Settings — bundle is scrubbed before submit', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    fireEvent.click(screen.getByTestId('valueos-nav-settings'));
    fireEvent.click(await screen.findByTestId('valueos-settings-report-bug'));
    fireEvent.change(await screen.findByTestId('valueos-bugreport-desc'), {
      target: { value: 'upload failed — reach me at me@example.com' },
    });
    fireEvent.click(screen.getByTestId('valueos-bugreport-submit'));
    await screen.findByTestId('valueos-bugreport-success');

    const svc = services.bugReport as MockBugReportService;
    expect(svc.submissions).toHaveLength(1);
    expect(svc.submissions[0].description).toContain('[EMAIL]'); // scrubbed
    expect(svc.submissions[0].description).not.toContain('me@example.com');
    expect(svc.submissions[0].metadata.tenant_id).toBe('tenant-acme');
  });

  it('reports a bug from the sidebar utility item — reachable on any screen, not just Settings', async () => {
    const { services } = makeServices('active');
    render(<ValueOsShell services={services} />);
    await loginToStorage();
    await storageToDashboard();
    // On the Dashboard (NOT Settings): the pinned sidebar "Report a bug" item opens the same
    // dialog directly — the fast path this placement exists to provide.
    expect(screen.getByTestId('valueos-dashboard')).toBeTruthy();
    fireEvent.click(screen.getByTestId('valueos-nav-report-bug'));
    fireEvent.change(await screen.findByTestId('valueos-bugreport-desc'), {
      target: { value: 'sidebar path works' },
    });
    fireEvent.click(screen.getByTestId('valueos-bugreport-submit'));
    await screen.findByTestId('valueos-bugreport-success');

    const svc = services.bugReport as MockBugReportService;
    expect(svc.submissions).toHaveLength(1);
    expect(svc.submissions[0].metadata.tenant_id).toBe('tenant-acme');
  });
});
