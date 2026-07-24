'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  ChevronLeft,
  Copy,
  Download,
  FileAudio,
  Pencil,
  Play,
  RefreshCw,
  Share2,
  Sparkles,
  Trash2,
  Users,
} from 'lucide-react';
import type { VmSegment } from './types';
import {
  deleteLocalRecording,
  exportLocalRecordingTranscript,
  fetchLocalRecordingTranscript,
  onRetranscribeComplete,
  onRetranscribeError,
  onRetranscribeProgress,
  promoteLocalRecordingToMeeting,
  renameLocalRecording,
  startRetranscribeLocalRecording,
  VmLocalRecording,
} from './tauriBridge';

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

function formatDuration(seconds?: number): string {
  if (!seconds || seconds <= 0) return '';
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  return `${m}m ${s}s`;
}

function formatTs(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

async function loadAudioBlobUrl(path: string): Promise<string> {
  const bytes = await invoke<number[]>('read_audio_file', { filePath: path });
  const ext = path.split('.').pop()?.toLowerCase();
  const mime = ext === 'wav' ? 'audio/wav' : 'audio/mp4';
  const blob = new Blob([new Uint8Array(bytes)], { type: mime });
  return URL.createObjectURL(blob);
}

export function RecordingDetailScreen({
  recording,
  onBack,
  onOpenMeeting,
}: {
  recording: VmLocalRecording;
  onBack: () => void;
  onOpenMeeting: (meetingId: string) => void;
}) {
  const [title, setTitle] = useState(recording.title);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState(recording.title);
  const [segments, setSegments] = useState<VmSegment[]>([]);
  const [loadingTranscript, setLoadingTranscript] = useState(true);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [audioError, setAudioError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [retranscribing, setRetranscribing] = useState(false);
  const [retranscribeMsg, setRetranscribeMsg] = useState('');
  const [retranscribeProgress, setRetranscribeProgress] = useState(0);
  const [toast, setToast] = useState<string | null>(null);

  const hasAudio = recording.hasAudio;
  const cancelledRef = useRef(false);

  useEffect(() => {
    fetchLocalRecordingTranscript(recording.folderName).then((s) => {
      setSegments(s);
      setLoadingTranscript(false);
    });
    return () => {
      cancelledRef.current = true;
    };
  }, [recording.folderName]);

  useEffect(() => {
    if (!retranscribing) return;
    let unProgress: (() => void) | undefined;
    let unComplete: (() => void) | undefined;
    let unError: (() => void) | undefined;

    onRetranscribeProgress((e) => {
      setRetranscribeProgress(e.progress_percentage);
      setRetranscribeMsg(e.message);
    }).then((u) => (unProgress = u));
    onRetranscribeComplete(async () => {
      setRetranscribing(false);
      setSegments(await fetchLocalRecordingTranscript(recording.folderName));
      setToast('Retranscribed successfully');
    }).then((u) => (unComplete = u));
    onRetranscribeError((err) => {
      setRetranscribing(false);
      setToast(`Retranscribe failed: ${err}`);
    }).then((u) => (unError = u));

    return () => {
      unProgress?.();
      unComplete?.();
      unError?.();
    };
  }, [retranscribing, recording.folderName]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3000);
    return () => clearTimeout(t);
  }, [toast]);

  const transcriptText = segments.map((s) => s.text).join('\n');

  const saveTitle = useCallback(async () => {
    const next = titleDraft.trim();
    setEditingTitle(false);
    if (!next || next === title) return;
    setTitle(next);
    await renameLocalRecording(recording.folderName, next);
  }, [titleDraft, title, recording.folderName]);

  const copyAll = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(transcriptText);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setToast('Copy failed');
    }
  }, [transcriptText]);

  const shareAll = useCallback(async () => {
    try {
      if (navigator.share) {
        await navigator.share({ title, text: transcriptText });
      } else {
        await navigator.clipboard.writeText(transcriptText);
        setToast('Sharing not available — copied instead');
      }
    } catch {
      /* user cancelled share sheet - not an error */
    }
  }, [title, transcriptText]);

  const exportFile = useCallback(
    async (format: 'txt' | 'srt') => {
      setBusy('export');
      const path = await exportLocalRecordingTranscript(recording.folderName, format);
      setBusy(null);
      setToast(path ? `Saved to Downloads` : 'Export failed');
    },
    [recording.folderName]
  );

  const playAudio = useCallback(async () => {
    if (!recording.audioPath) return;
    setAudioError(null);
    try {
      setAudioUrl(await loadAudioBlobUrl(recording.audioPath));
    } catch (e) {
      setAudioError(String(e));
    }
  }, [recording.audioPath]);

  const retranscribe = useCallback(async () => {
    setRetranscribeProgress(0);
    setRetranscribeMsg('Starting…');
    setRetranscribing(true);
    try {
      await startRetranscribeLocalRecording(recording.folderName);
    } catch (e) {
      setRetranscribing(false);
      setToast(`Couldn't start retranscribe: ${String(e)}`);
    }
  }, [recording.folderName]);

  const addToMeetings = useCallback(async () => {
    setBusy('promote');
    const meetingId = await promoteLocalRecordingToMeeting(recording.folderName);
    setBusy(null);
    if (meetingId) {
      setToast('Added to Meetings');
    } else {
      setToast('Failed to add to Meetings');
    }
    return meetingId;
  }, [recording.folderName]);

  const summarize = useCallback(async () => {
    setBusy('summarize');
    const meetingId = await promoteLocalRecordingToMeeting(recording.folderName);
    setBusy(null);
    if (meetingId) {
      onOpenMeeting(meetingId);
    } else {
      setToast('Failed to prepare summary');
    }
  }, [recording.folderName, onOpenMeeting]);

  const doDelete = useCallback(async () => {
    setBusy('delete');
    const ok = await deleteLocalRecording(recording.folderName);
    setBusy(null);
    if (ok) {
      onBack();
    } else {
      setToast('Delete failed');
    }
  }, [recording.folderName, onBack]);

  return (
    <div className="col f1" style={{ height: '100%' }}>
      <div className="appbar" style={{ padding: '8px 6px 0' }}>
        <button className="iconbtn" onClick={onBack}>
          <ChevronLeft size={22} strokeWidth={2.2} />
        </button>
        {editingTitle ? (
          <input
            autoFocus
            value={titleDraft}
            onChange={(e) => setTitleDraft(e.target.value)}
            onBlur={saveTitle}
            onKeyDown={(e) => e.key === 'Enter' && saveTitle()}
            style={{ flex: 1, fontSize: 16, fontWeight: 700 }}
          />
        ) : (
          <h1
            style={{ fontSize: 16, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
          >
            {title}
          </h1>
        )}
        <button
          className="iconbtn"
          onClick={() => {
            setTitleDraft(title);
            setEditingTitle((v) => !v);
          }}
        >
          <Pencil size={18} strokeWidth={2} />
        </button>
      </div>

      <div className="row" style={{ padding: '0 20px 10px' }}>
        <span className="muted mono fs12">
          {formatDateLine(recording.createdAt)}
          {recording.durationSeconds ? ` · ${formatDuration(recording.durationSeconds)}` : ''}
          {recording.status && recording.status !== 'completed' ? ` · ${recording.status}` : ''}
        </span>
      </div>

      <div className="row gap8 wrap" style={{ padding: '0 16px 14px' }}>
        {hasAudio && !audioUrl && (
          <button className="btn btns sm" onClick={playAudio}>
            <Play size={14} style={{ marginRight: 6 }} />
            Play
          </button>
        )}
        <button className="btn btns sm" onClick={copyAll} disabled={segments.length === 0}>
          <Copy size={14} style={{ marginRight: 6 }} />
          {copied ? 'Copied!' : 'Copy'}
        </button>
        <button className="btn btns sm" onClick={shareAll} disabled={segments.length === 0}>
          <Share2 size={14} style={{ marginRight: 6 }} />
          Share
        </button>
        <button className="btn btns sm" onClick={() => exportFile('txt')} disabled={busy === 'export' || segments.length === 0}>
          <Download size={14} style={{ marginRight: 6 }} />
          Export .txt
        </button>
        <button className="btn btns sm" onClick={() => exportFile('srt')} disabled={busy === 'export' || segments.length === 0}>
          <Download size={14} style={{ marginRight: 6 }} />
          Export .srt
        </button>
        {hasAudio && (
          <button className="btn btns sm" onClick={retranscribe} disabled={retranscribing}>
            <RefreshCw size={14} style={{ marginRight: 6 }} />
            Retranscribe
          </button>
        )}
        <button className="btn btns sm" onClick={summarize} disabled={busy === 'summarize' || segments.length === 0}>
          <Sparkles size={14} style={{ marginRight: 6 }} />
          Summarize
        </button>
        <button className="btn btns sm" onClick={addToMeetings} disabled={busy === 'promote'}>
          <Users size={14} style={{ marginRight: 6 }} />
          Add to Meetings
        </button>
        <button
          className="btn btnd sm"
          onClick={() => setConfirmDelete(true)}
          disabled={busy === 'delete'}
        >
          <Trash2 size={14} style={{ marginRight: 6 }} />
          Delete
        </button>
      </div>

      {audioUrl && (
        <div style={{ padding: '0 20px 14px' }}>
          {/* eslint-disable-next-line jsx-a11y/media-has-caption */}
          <audio controls src={audioUrl} style={{ width: '100%' }} />
        </div>
      )}
      {audioError && (
        <p className="muted fs12" style={{ padding: '0 20px 10px' }}>
          Couldn&apos;t load audio: {audioError}
        </p>
      )}

      {retranscribing && (
        <div className="card col ac jc gap10" style={{ margin: '0 20px 14px', padding: '18px 16px' }}>
          <span className="mono fs13 muted">{retranscribeMsg || 'Working…'}</span>
          <div className="progress-track" style={{ width: '100%' }}>
            <div className="progress-fill" style={{ width: `${retranscribeProgress}%` }} />
          </div>
        </div>
      )}

      {toast && (
        <div style={{ padding: '0 20px 10px' }}>
          <span className="pill" style={{ background: 'hsl(var(--accent))', color: 'hsl(var(--accent-fg))' }}>
            {toast}
          </span>
        </div>
      )}

      <div className="content" style={{ padding: '0 20px 24px' }}>
        {loadingTranscript && <p className="muted fs13">Loading transcript…</p>}
        {!loadingTranscript && segments.length === 0 && (
          <div className="col ac jc" style={{ padding: '40px 12px', textAlign: 'center', gap: 10 }}>
            <FileAudio size={32} color="hsl(var(--muted-fg, var(--fg)))" strokeWidth={1.5} />
            <p className="muted fs13">
              {hasAudio ? 'No transcript segments were captured.' : 'No transcript or audio for this recording.'}
            </p>
          </div>
        )}
        {segments.map((seg) => (
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

      {confirmDelete && (
        <div className="sheet-backdrop" onClick={() => setConfirmDelete(false)}>
          <div className="card col gap14 sheet" onClick={(e) => e.stopPropagation()}>
            <div className="sheet-grip" />
            <h2 style={{ margin: 0, fontSize: 17, fontWeight: 800 }}>Delete this recording?</h2>
            <p className="muted fs13" style={{ margin: 0, lineHeight: 1.5 }}>
              This permanently deletes the audio file, transcript, and metadata for &quot;{title}&quot;. This can&apos;t be undone.
            </p>
            <button className="btn btnd lg" onClick={doDelete}>
              Delete permanently
            </button>
            <button className="btn btns md" onClick={() => setConfirmDelete(false)}>
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
