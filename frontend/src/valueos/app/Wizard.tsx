'use client';
// VALUEOS: the New-transcript wizard (UI_GUIDE §5) — a centered modal over a blurred scrim
// with 4 steps: tenant → lead/opportunity → record → name. Continue is disabled until the
// current step has a selection. "Start transcript" emits the chosen metadata; the flow then
// opens the Recording screen. This is the blocking-metadata rule (FEATURE-flow) in the new UI:
// recording cannot start until tenant + type + existing target + name are all set.
import React, { useEffect, useState } from 'react';
import { useValueOs } from '../context/ValueOsProvider';
import type { EntitledTenant } from '../auth/authService';
import type { ActivityType, Lead, Opportunity } from '../api/types';
import { ValueOsApiError } from '../api/types';
import { Avatar } from './parts';
import { IcArrowLeft, IcClose } from './icons';
import type { StartCallMeta } from './types';

interface Rec {
  id: string;
  label: string;
  meta: string;
}

function leadMeta(l: Lead): string {
  return [l.company, l.status].filter(Boolean).join(' · ') || 'Lead';
}
function oppMeta(o: Opportunity): string {
  const amount =
    o.amount != null ? `${o.currency ?? ''}${o.amount.toLocaleString()}`.trim() : null;
  return [o.stage, amount].filter(Boolean).join(' · ') || 'Opportunity';
}

export function Wizard({
  entitledTenants,
  onClose,
  onStart,
  onLostAccess,
}: {
  entitledTenants: EntitledTenant[];
  onClose: () => void;
  onStart: (meta: StartCallMeta) => void;
  onLostAccess: () => void;
}) {
  const { client } = useValueOs();
  const [step, setStep] = useState(0); // 0..3
  const [tenantId, setTenantId] = useState<string | null>(null);
  const [activityType, setActivityType] = useState<ActivityType | null>(null);
  const [targetId, setTargetId] = useState<string | null>(null);
  const [q, setQ] = useState('');
  const [records, setRecords] = useState<Rec[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [callName, setCallName] = useState('');
  const [nameTouched, setNameTouched] = useState(false);

  const tenant = entitledTenants.find((t) => t.tenant.id === tenantId) ?? null;
  const record = records.find((r) => r.id === targetId) ?? null;

  // Step 3: load the searchable, type-filtered record list.
  useEffect(() => {
    if (step !== 2 || !tenantId || !activityType) return;
    let cancelled = false;
    setLoading(true);
    setError('');
    (async () => {
      try {
        const res =
          activityType === 'lead'
            ? await client.listLeads(tenantId, { q })
            : await client.listOpportunities(tenantId, { q });
        if (cancelled) return;
        setRecords(
          res.items.map((t) =>
            activityType === 'lead'
              ? { id: t.id, label: t.label, meta: leadMeta(t as Lead) }
              : { id: t.id, label: t.label, meta: oppMeta(t as Opportunity) },
          ),
        );
      } catch (e) {
        if (cancelled) return;
        if (e instanceof ValueOsApiError && e.isNotEntitled) {
          onLostAccess();
          return;
        }
        setError((e as Error).message);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, step, tenantId, activityType, q, onLostAccess]);

  // Step 4: default the name from the chosen record; stop overwriting once the user edits it.
  useEffect(() => {
    if (nameTouched) return;
    setCallName(record ? `Discovery Call — ${record.label}` : '');
  }, [record, nameTouched]);

  const canContinue =
    (step === 0 && !!tenantId) ||
    (step === 1 && !!activityType) ||
    (step === 2 && !!targetId) ||
    step === 3;

  const back = () => {
    if (step === 0) onClose();
    else setStep((s) => s - 1);
  };

  const next = () => {
    if (!canContinue) return;
    if (step < 3) setStep((s) => s + 1);
    else start();
  };

  const start = () => {
    if (!tenant || !activityType || !record) return;
    onStart({
      tenantId: tenant.tenant.id,
      tenantName: tenant.tenant.name,
      activityType,
      targetId: record.id,
      targetLabel: record.label,
      callName: callName.trim() || `Discovery Call — ${record.label}`,
    });
  };

  return (
    <div className="va-scrim va-root" data-testid="valueos-wizard" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="va-modal" role="dialog" aria-modal="true">
        <div className="va-modal-head">
          <div className="va-ovl">New transcript</div>
          <button className="va-btn va-btn-ghost-light va-btn-sm" data-testid="valueos-wizard-close" onClick={onClose} aria-label="Close" style={{ padding: 8, borderRadius: 8 }}>
            <IcClose size={16} />
          </button>
        </div>
        <div className="va-seg" aria-hidden="true">
          {[0, 1, 2, 3].map((i) => (
            <i key={i} className={i <= step ? 'on' : ''} />
          ))}
        </div>

        <div className="va-modal-body va-scroll" style={{ minHeight: 240 }}>
          {step === 0 && (
            <>
              <h3 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 20, margin: '4px 0 12px' }}>
                Select a tenant
              </h3>
              {entitledTenants.map((t) => (
                <button
                  key={t.tenant.id}
                  className={`va-choice${tenantId === t.tenant.id ? ' on' : ''}`}
                  data-testid={`valueos-wizard-tenant-${t.tenant.id}`}
                  onClick={() => {
                    setTenantId(t.tenant.id);
                    setTargetId(null);
                    setRecords([]);
                  }}
                  style={{ display: 'flex', alignItems: 'center', gap: 12 }}
                >
                  <Avatar name={t.tenant.name} size={36} />
                  <span>
                    <span style={{ display: 'block', fontWeight: 700 }}>{t.tenant.name}</span>
                    <span className="va-muted" style={{ fontSize: 13 }}>
                      {t.tenant.role || 'Workspace'}
                    </span>
                  </span>
                </button>
              ))}
            </>
          )}

          {step === 1 && (
            <>
              <h3 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 20, margin: '4px 0 12px' }}>
                Lead or opportunity?
              </h3>
              <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10 }}>
                {(['lead', 'opportunity'] as ActivityType[]).map((k) => (
                  <button
                    key={k}
                    className={`va-choice${activityType === k ? ' on' : ''}`}
                    data-testid={`valueos-wizard-type-${k}`}
                    style={{ marginBottom: 0, textAlign: 'center', padding: '22px 14px' }}
                    onClick={() => {
                      setActivityType(k);
                      setTargetId(null);
                      setRecords([]);
                    }}
                  >
                    <span style={{ display: 'block', fontWeight: 800, fontSize: 17, textTransform: 'capitalize' }}>{k}</span>
                    <span className="va-muted" style={{ fontSize: 13 }}>
                      {k === 'lead' ? 'An early-stage contact' : 'An active deal in your pipeline'}
                    </span>
                  </button>
                ))}
              </div>
            </>
          )}

          {step === 2 && (
            <>
              <h3 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 20, margin: '4px 0 10px' }}>
                Select {activityType === 'lead' ? 'a lead' : 'an opportunity'}
              </h3>
              <input
                className="va-input"
                data-testid="valueos-wizard-search"
                placeholder="Search…"
                value={q}
                onChange={(e) => setQ(e.target.value)}
                style={{ marginBottom: 10 }}
              />
              <div data-testid="valueos-wizard-records">
                {loading && <p className="va-muted">Loading…</p>}
                {!loading &&
                  records.map((r) => (
                    <button
                      key={r.id}
                      className={`va-choice${targetId === r.id ? ' on' : ''}`}
                      data-testid={`valueos-wizard-record-${r.id}`}
                      onClick={() => setTargetId(r.id)}
                    >
                      <span style={{ display: 'block', fontWeight: 700 }}>{r.label}</span>
                      <span className="va-muted" style={{ fontSize: 13 }}>{r.meta}</span>
                    </button>
                  ))}
                {!loading && records.length === 0 && !error && <p className="va-muted">No matches.</p>}
                {error && <p style={{ color: 'var(--va-signal-red)' }} data-testid="valueos-wizard-error">{error}</p>}
              </div>
            </>
          )}

          {step === 3 && (
            <>
              <h3 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 20, margin: '4px 0 12px' }}>
                Name this call
              </h3>
              <input
                className="va-input"
                data-testid="valueos-wizard-name"
                value={callName}
                placeholder="Name this call"
                onChange={(e) => {
                  setCallName(e.target.value);
                  setNameTouched(true);
                }}
              />
              <div className="va-card" style={{ marginTop: 16, padding: 14, boxShadow: 'none' }}>
                <div className="va-ovl" style={{ marginBottom: 8 }}>Summary</div>
                <SummaryRow k="Tenant" v={tenant?.tenant.name ?? '—'} />
                <SummaryRow k="Type" v={activityType ? activityType[0].toUpperCase() + activityType.slice(1) : '—'} />
                <SummaryRow k="Record" v={record?.label ?? '—'} />
              </div>
            </>
          )}
        </div>

        <div className="va-modal-foot">
          <button className="va-btn va-btn-ghost-light" data-testid="valueos-wizard-back" onClick={back}>
            {step === 0 ? 'Cancel' : (<><IcArrowLeft size={15} /> Back</>)}
          </button>
          {step < 3 ? (
            <button className="va-btn va-btn-primary" data-testid="valueos-wizard-continue" disabled={!canContinue} onClick={next}>
              Continue
            </button>
          ) : (
            <button className="va-btn va-btn-danger" data-testid="valueos-wizard-start" onClick={next}>
              <span className="va-dot va-dot-red" /> Start transcript
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function SummaryRow({ k, v }: { k: string; v: string }) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', padding: '5px 0', fontSize: 14 }}>
      <span className="va-muted">{k}</span>
      <span style={{ fontWeight: 600 }}>{v}</span>
    </div>
  );
}
