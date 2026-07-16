import React, { useEffect, useState } from 'react';
import type { CSSProperties } from 'react';
import { useValueOs } from '../../context/ValueOsProvider';
// alias import so tests can vi.mock this module cleanly
import { useRecordingController } from '@/valueos/capture/useRecordingController';
import type { EntitledTenant } from '../../auth/authService';
import type { ActivityType, Lead, Opportunity } from '../../api/types';
import { ValueOsApiError } from '../../api/types';
import type { CaptureResult } from '../flowTypes';
import * as ui from './ui';

// VALUEOS: capture screen with FULLY BLOCKING pre-recording metadata. START is enabled
// ONLY when tenant + activity type + a specific existing target are all chosen. Attaches
// to EXISTING leads/opportunities only (never creates). Recording reuses upstream
// transcription via useRecordingController.
interface TargetItem {
  id: string;
  label: string;
}

export function CaptureScreen({
  entitledTenants,
  onFinish,
  onLostAccess,
}: {
  entitledTenants: EntitledTenant[];
  onFinish: (r: CaptureResult) => void;
  onLostAccess: () => void;
}) {
  const { client } = useValueOs();
  const rec = useRecordingController();
  const [tenantId, setTenantId] = useState<string | null>(null);
  const [activityType, setActivityType] = useState<ActivityType | null>(null);
  const [targetId, setTargetId] = useState<string | null>(null);
  const [q, setQ] = useState('');
  const [targets, setTargets] = useState<TargetItem[]>([]);
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState('');

  // Load the (searchable) target list whenever tenant/type/search changes.
  useEffect(() => {
    let cancelled = false;
    if (!tenantId || !activityType) {
      setTargets([]);
      return;
    }
    (async () => {
      try {
        const res =
          activityType === 'lead'
            ? await client.listLeads(tenantId, { q })
            : await client.listOpportunities(tenantId, { q });
        if (!cancelled)
          setTargets(res.items.map((t: Lead | Opportunity) => ({ id: t.id, label: t.label })));
      } catch (e) {
        if (cancelled) return;
        // §2.7: a 403 feat_agent means this workspace just lost the add-on → re-run the
        // gate (drops it from the picker / blocks if none remain) rather than show a
        // dead-end error while the tenant stays selectable.
        if (e instanceof ValueOsApiError && e.isNotEntitled) {
          onLostAccess();
          return;
        }
        setError((e as Error).message);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, tenantId, activityType, q]);

  const target = targets.find((t) => t.id === targetId) ?? null;
  const tenantName = entitledTenants.find((t) => t.tenant.id === tenantId)?.tenant.name ?? '';
  const canStart = !!tenantId && !!activityType && !!target;

  const start = async () => {
    if (!canStart) return;
    setRecording(true);
    await rec.start(`${target!.label} — ${new Date().toISOString()}`);
  };

  const stop = async () => {
    const transcriptText = await rec.stop();
    onFinish({
      tenantId: tenantId!,
      tenantName: tenantName || tenantId!,
      activityType: activityType!,
      targetId: target!.id,
      targetLabel: target!.label,
      transcriptText,
    });
  };

  if (recording) {
    return (
      <div data-testid="valueos-capture-recording" style={ui.page}>
        <div style={ui.card}>
          <h1 style={ui.h1}>Recording…</h1>
          <p style={ui.sub}>
            Attaching to <strong>{target?.label}</strong>. Transcription runs locally on
            this device.
          </p>
          <button data-testid="valueos-capture-stop" style={ui.primaryBtn} onClick={stop}>
            Stop &amp; finish
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      data-testid="valueos-capture"
      style={{ ...ui.page, justifyContent: 'flex-start', padding: '56px 24px 24px', overflowY: 'auto' }}
    >
      <div style={{ ...ui.card, maxWidth: 560, alignItems: 'stretch' }}>
        <h1 style={{ ...ui.h1, textAlign: 'center' }}>Attach this meeting</h1>
        <p style={{ ...ui.sub, textAlign: 'center' }}>
          Recording can&apos;t start until you choose the tenant, the type, and the exact
          lead or opportunity to attach the transcript to.
        </p>

        <label style={label}>Tenant</label>
        <select
          data-testid="valueos-capture-tenant"
          style={field}
          value={tenantId ?? ''}
          onChange={(e) => {
            setTenantId(e.target.value || null);
            setTargetId(null);
          }}
        >
          <option value="">Select a tenant…</option>
          {entitledTenants.map((t) => (
            <option key={t.tenant.id} value={t.tenant.id}>
              {t.tenant.name}
            </option>
          ))}
        </select>

        <label style={label}>Activity type</label>
        <div style={{ display: 'flex', gap: 8 }}>
          <button
            data-testid="valueos-capture-type-lead"
            style={activityType === 'lead' ? chipOn : chip}
            onClick={() => {
              setActivityType('lead');
              setTargetId(null);
            }}
          >
            Lead
          </button>
          <button
            data-testid="valueos-capture-type-opportunity"
            style={activityType === 'opportunity' ? chipOn : chip}
            onClick={() => {
              setActivityType('opportunity');
              setTargetId(null);
            }}
          >
            Opportunity
          </button>
        </div>

        {tenantId && activityType && (
          <>
            <label style={label}>{activityType === 'lead' ? 'Lead' : 'Opportunity'}</label>
            <input
              data-testid="valueos-capture-search"
              style={field}
              placeholder="Search…"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
            <div data-testid="valueos-capture-targets" style={{ maxHeight: 200, overflowY: 'auto' }}>
              {targets.map((t) => (
                <button
                  key={t.id}
                  data-testid={`valueos-capture-target-${t.id}`}
                  style={targetId === t.id ? listItemOn : listItem}
                  onClick={() => setTargetId(t.id)}
                >
                  {t.label}
                </button>
              ))}
              {targets.length === 0 && <p style={ui.sub}>No matches.</p>}
            </div>
          </>
        )}

        {error && (
          <p data-testid="valueos-capture-error" style={{ ...ui.sub, color: '#ffd7d7' }}>
            {error}
          </p>
        )}

        <button
          data-testid="valueos-capture-start"
          style={canStart ? ui.primaryBtn : ui.primaryBtnDisabled}
          disabled={!canStart}
          onClick={start}
        >
          Start recording
        </button>
      </div>
    </div>
  );
}

const label: CSSProperties = { fontSize: 13, fontWeight: 700, opacity: 0.85, margin: '16px 0 6px' };
const field: CSSProperties = {
  width: '100%',
  padding: '11px 12px',
  borderRadius: 8,
  border: '1px solid rgba(255,255,255,0.35)',
  background: 'rgba(255,255,255,0.1)',
  color: '#fff',
  fontSize: 14,
};
const chip: CSSProperties = {
  flex: 1,
  padding: '10px 0',
  borderRadius: 8,
  border: '1px solid rgba(255,255,255,0.35)',
  background: 'transparent',
  color: '#fff',
  fontWeight: 600,
  cursor: 'pointer',
};
const chipOn: CSSProperties = { ...chip, background: '#fff', color: '#0030BC' };
const listItem: CSSProperties = {
  display: 'block',
  width: '100%',
  textAlign: 'left',
  padding: '10px 12px',
  borderRadius: 8,
  border: '1px solid transparent',
  background: 'rgba(255,255,255,0.08)',
  color: '#fff',
  cursor: 'pointer',
  marginBottom: 6,
};
const listItemOn: CSSProperties = { ...listItem, border: '1px solid #fff', background: 'rgba(255,255,255,0.2)' };
