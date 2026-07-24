'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import { AlertCircle, ChevronLeft } from 'lucide-react';
import type { VmSummaryStatus } from './types';
import { VM_TEMPLATES } from './types';
import {
  fetchMeetingDetail,
  fetchSummary,
  generateSummary,
  MeetingDetail,
} from './tauriBridge';

type Tab = 'transcript' | 'summary' | 'notes';

function formatTs(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

function formatDateLine(iso?: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return (
    d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' }) +
    ' · ' +
    d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
  );
}

interface Sections {
  keyPoints: string[];
  actionItems: { text: string; done: boolean }[];
  decisions: string[];
  other: string[];
}

/** Parse the "## Key points / ## Action items / ## Decisions" markdown shape. */
function parseSections(md: string): Sections {
  const out: Sections = { keyPoints: [], actionItems: [], decisions: [], other: [] };
  let cur = '';
  for (const line of (md || '').split('\n')) {
    const t = line.trim();
    if (t.startsWith('#')) {
      cur = t.replace(/^#+\s*/, '').toLowerCase();
      continue;
    }
    if (!t) continue;
    if (cur.startsWith('key')) {
      if (t.startsWith('- ')) out.keyPoints.push(t.slice(2));
    } else if (cur.startsWith('action')) {
      if (t.startsWith('- [ ]')) out.actionItems.push({ text: t.slice(5).trim(), done: false });
      else if (t.toLowerCase().startsWith('- [x]'))
        out.actionItems.push({ text: t.slice(5).trim(), done: true });
      else if (t.startsWith('- ')) out.actionItems.push({ text: t.slice(2), done: false });
    } else if (cur.startsWith('decision')) {
      if (t.startsWith('- ')) out.decisions.push(t.slice(2));
    } else {
      out.other.push(t);
    }
  }
  return out;
}

const NOTES_KEY = (id: string) => `vm-notes-${id}`;

export function MeetingDetailScreen({
  meetingId,
  initialTab,
  onBack,
  onOpenSettings,
}: {
  meetingId: string;
  initialTab: Tab;
  onBack: () => void;
  onOpenSettings: () => void;
}) {
  const [tab, setTab] = useState<Tab>(initialTab);
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [summaryStatus, setSummaryStatus] = useState<VmSummaryStatus>('idle');
  const [summaryMd, setSummaryMd] = useState('');
  const [template, setTemplate] = useState('standup');
  const [showPicker, setShowPicker] = useState(false);
  const [notes, setNotes] = useState('');
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    fetchMeetingDetail(meetingId).then(setDetail);
    fetchSummary(meetingId).then((s) => {
      if (s.status === 'ready') {
        setSummaryMd(s.markdown);
        setSummaryStatus('ready');
      }
    });
    try {
      setNotes(window.localStorage.getItem(NOTES_KEY(meetingId)) ?? '');
    } catch {
      /* ignore */
    }
  }, [meetingId]);

  const saveNotes = useCallback(
    (v: string) => {
      setNotes(v);
      try {
        window.localStorage.setItem(NOTES_KEY(meetingId), v);
      } catch {
        /* ignore */
      }
    },
    [meetingId]
  );

  const transcriptText = useMemo(
    () => (detail?.segments ?? []).map((s) => s.text).join('\n'),
    [detail]
  );

  const generate = useCallback(
    async (templateId?: string) => {
      const tid = templateId ?? template;
      setTemplate(tid);
      setShowPicker(false);
      setSummaryStatus('generating');
      try {
        await generateSummary(meetingId, transcriptText, tid);
        // Poll until the summary lands (processing is asynchronous)
        for (let i = 0; i < 60; i++) {
          await new Promise((r) => setTimeout(r, 3000));
          const s = await fetchSummary(meetingId);
          if (s.status === 'ready' && s.markdown !== summaryMd) {
            setSummaryMd(s.markdown);
            setSummaryStatus('ready');
            return;
          }
        }
        setSummaryStatus('error');
      } catch (e) {
        console.warn('[vm] summary generation failed', e);
        setSummaryStatus('error');
      }
    },
    [meetingId, template, transcriptText, summaryMd]
  );

  const copyAll = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(transcriptText);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* ignore */
    }
  }, [transcriptText]);

  const sections = useMemo(() => parseSections(summaryMd), [summaryMd]);

  return (
    <div className="col f1" style={{ height: '100%' }}>
      <div className="appbar" style={{ padding: '8px 6px 0' }}>
        <button className="iconbtn" onClick={onBack}>
          <ChevronLeft size={22} strokeWidth={2.2} />
        </button>
        <h1 style={{ fontSize: 16 }}>{detail?.title ?? '…'}</h1>
      </div>
      {detail?.created_at && (
        <div className="row" style={{ padding: '0 20px 12px' }}>
          <span className="muted mono fs12">{formatDateLine(detail.created_at)}</span>
        </div>
      )}

      <div style={{ padding: '0 16px 10px' }}>
        <div className="seg">
          {(['transcript', 'summary', 'notes'] as Tab[]).map((t) => (
            <button key={t} className={tab === t ? 'on' : ''} onClick={() => setTab(t)}>
              {t.charAt(0).toUpperCase() + t.slice(1)}
            </button>
          ))}
        </div>
      </div>

      <div className="content" style={{ paddingBottom: 24 }}>
        {tab === 'transcript' && (
          <div className="col" style={{ padding: '0 20px 10px' }}>
            <div className="row gap8" style={{ padding: '6px 0 12px' }}>
              <button className="btn btnghost sm" onClick={copyAll}>
                {copied ? 'Copied!' : 'Copy all'}
              </button>
            </div>
            {(detail?.segments ?? []).length === 0 && (
              <p className="muted fs13">No transcript was captured for this meeting.</p>
            )}
            {(detail?.segments ?? []).map((seg) => (
              <div
                key={seg.id}
                className="row gap10"
                style={{ padding: '8px 0', alignItems: 'flex-start', borderBottom: '1px solid hsl(var(--border))' }}
              >
                <span className="mono muted fs11" style={{ paddingTop: 2, flexShrink: 0, width: 38 }}>
                  {formatTs(seg.timestamp)}
                </span>
                <span className="fs14" style={{ lineHeight: 1.55 }}>
                  {seg.text}
                </span>
              </div>
            ))}
          </div>
        )}

        {tab === 'summary' && (
          <div className="col" style={{ padding: '0 20px 10px', gap: 14 }}>
            {summaryStatus === 'idle' && (
              <div className="card col gap14" style={{ padding: 18 }}>
                <span style={{ fontWeight: 700, fontSize: 15 }}>Generate a summary</span>
                <p className="muted fs13" style={{ margin: 0, lineHeight: 1.5 }}>
                  Pick a template that matches this meeting.
                </p>
                <div className="row gap8 wrap">
                  {VM_TEMPLATES.map((t) => (
                    <button
                      key={t.id}
                      className={`chip ${t.id === template ? 'on' : ''}`}
                      onClick={() => setTemplate(t.id)}
                    >
                      {t.name}
                    </button>
                  ))}
                </div>
                <button className="btn btnp md" onClick={() => generate()}>
                  Generate summary
                </button>
              </div>
            )}

            {summaryStatus === 'generating' && (
              <div className="card col ac jc gap12" style={{ padding: '32px 20px' }}>
                <div className="mono fs13 muted">Summarizing…</div>
                <div className="col gap8" style={{ width: '100%' }}>
                  <div style={{ height: 10, borderRadius: 6, background: 'hsl(var(--muted))' }} />
                  <div style={{ height: 10, borderRadius: 6, background: 'hsl(var(--muted))', width: '85%' }} />
                  <div style={{ height: 10, borderRadius: 6, background: 'hsl(var(--muted))', width: '60%' }} />
                </div>
              </div>
            )}

            {summaryStatus === 'error' && (
              <div className="card col ac gap10" style={{ padding: '26px 20px', textAlign: 'center' }}>
                <AlertCircle size={34} color="hsl(var(--destructive))" strokeWidth={1.8} />
                <span style={{ fontWeight: 700, fontSize: 15 }}>Couldn&apos;t generate a summary</span>
                <p className="muted fs13" style={{ margin: 0, lineHeight: 1.5 }}>
                  Check your summary provider setup in Settings, then try again.
                </p>
                <div className="row gap8">
                  <button className="btn btns md" onClick={() => generate()}>
                    Try again
                  </button>
                  <button className="btn btnp md" onClick={onOpenSettings}>
                    Open Settings
                  </button>
                </div>
              </div>
            )}

            {summaryStatus === 'ready' && (
              <div className="col gap14">
                <button
                  className="btn btnghost sm"
                  style={{ alignSelf: 'flex-end' }}
                  onClick={() => setShowPicker(true)}
                >
                  Regenerate
                </button>
                {sections.keyPoints.length > 0 && (
                  <div className="col gap8">
                    <span className="fw8 fs13" style={{ letterSpacing: '0.02em' }}>KEY POINTS</span>
                    {sections.keyPoints.map((t, i) => (
                      <div key={i} className="row gap8" style={{ alignItems: 'flex-start' }}>
                        <span style={{ color: 'hsl(var(--primary))' }}>•</span>
                        <span className="fs14" style={{ lineHeight: 1.5 }}>{t}</span>
                      </div>
                    ))}
                  </div>
                )}
                {sections.actionItems.length > 0 && (
                  <div className="col gap8">
                    <span className="fw8 fs13" style={{ letterSpacing: '0.02em' }}>ACTION ITEMS</span>
                    {sections.actionItems.map((a, i) => (
                      <div key={i} className="row gap10" style={{ alignItems: 'flex-start' }}>
                        <div
                          style={{
                            width: 16,
                            height: 16,
                            borderRadius: 4,
                            border: '1.6px solid hsl(var(--primary))',
                            flexShrink: 0,
                            marginTop: 2,
                            background: a.done ? 'hsl(var(--primary))' : 'transparent',
                          }}
                        />
                        <span className="fs14" style={{ lineHeight: 1.5 }}>{a.text}</span>
                      </div>
                    ))}
                  </div>
                )}
                {sections.decisions.length > 0 && (
                  <div className="col gap8">
                    <span className="fw8 fs13" style={{ letterSpacing: '0.02em' }}>DECISIONS</span>
                    {sections.decisions.map((t, i) => (
                      <div key={i} className="row gap8" style={{ alignItems: 'flex-start' }}>
                        <span style={{ color: 'hsl(var(--primary))' }}>•</span>
                        <span className="fs14" style={{ lineHeight: 1.5 }}>{t}</span>
                      </div>
                    ))}
                  </div>
                )}
                {sections.keyPoints.length === 0 &&
                  sections.actionItems.length === 0 &&
                  sections.decisions.length === 0 && (
                    <div className="col gap8">
                      {sections.other.map((t, i) => (
                        <p key={i} className="fs14" style={{ margin: 0, lineHeight: 1.55 }}>{t}</p>
                      ))}
                    </div>
                  )}
              </div>
            )}
          </div>
        )}

        {tab === 'notes' && (
          <div className="col gap10" style={{ padding: '0 20px 10px' }}>
            <textarea
              rows={12}
              placeholder="Add your notes…"
              value={notes}
              onChange={(e) => saveNotes(e.target.value)}
              style={{ resize: 'none' }}
            />
          </div>
        )}
      </div>

      {showPicker && (
        <div className="sheet-backdrop" onClick={() => setShowPicker(false)}>
          <div className="card col gap12 sheet" onClick={(e) => e.stopPropagation()}>
            <div className="sheet-grip" />
            <span style={{ fontWeight: 800, fontSize: 16 }}>Regenerate with template</span>
            {VM_TEMPLATES.map((t) => (
              <button
                key={t.id}
                className="btn md"
                style={{
                  width: '100%',
                  justifyContent: 'flex-start',
                  background: t.id === template ? 'hsl(var(--accent))' : 'transparent',
                  color: t.id === template ? 'hsl(var(--accent-fg))' : 'hsl(var(--fg))',
                }}
                onClick={() => generate(t.id)}
              >
                {t.name}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
