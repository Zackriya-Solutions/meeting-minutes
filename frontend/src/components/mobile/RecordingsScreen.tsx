'use client';

import { useCallback, useEffect, useState } from 'react';
import { FileAudio, Play, RefreshCw } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import type { VmSegment } from './types';
import { fetchLocalRecordings, fetchLocalRecordingTranscript, VmLocalRecording } from './tauriBridge';

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

/** Reads a local audio file through Tauri and returns a playable blob: URL. */
async function loadAudioBlobUrl(path: string): Promise<string> {
  const bytes = await invoke<number[]>('read_audio_file', { filePath: path });
  const ext = path.split('.').pop()?.toLowerCase();
  const mime = ext === 'wav' ? 'audio/wav' : ext === 'm4a' ? 'audio/mp4' : 'audio/mp4';
  const blob = new Blob([new Uint8Array(bytes)], { type: mime });
  return URL.createObjectURL(blob);
}

export function RecordingsScreen() {
  const [recordings, setRecordings] = useState<VmLocalRecording[]>([]);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [segments, setSegments] = useState<VmSegment[]>([]);
  const [segmentsLoading, setSegmentsLoading] = useState(false);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [audioError, setAudioError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setRecordings(await fetchLocalRecordings());
    setLoading(false);
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const toggleExpand = useCallback(
    async (rec: VmLocalRecording) => {
      if (expanded === rec.folderName) {
        setExpanded(null);
        if (audioUrl) URL.revokeObjectURL(audioUrl);
        setAudioUrl(null);
        setAudioError(null);
        return;
      }
      setExpanded(rec.folderName);
      setAudioUrl(null);
      setAudioError(null);
      if (rec.hasTranscript) {
        setSegmentsLoading(true);
        setSegments(await fetchLocalRecordingTranscript(rec.folderName));
        setSegmentsLoading(false);
      } else {
        setSegments([]);
      }
    },
    [expanded, audioUrl]
  );

  const playAudio = useCallback(async (path: string) => {
    setAudioError(null);
    try {
      setAudioUrl(await loadAudioBlobUrl(path));
    } catch (e) {
      setAudioError(String(e));
    }
  }, []);

  return (
    <div className="col f1" style={{ height: '100%', overflow: 'hidden' }}>
      <div className="appbar" style={{ padding: '10px 16px 4px' }}>
        <h1>Recordings</h1>
        <button className="iconbtn" onClick={load} aria-label="Refresh">
          <RefreshCw size={19} strokeWidth={2} />
        </button>
      </div>
      <p className="muted fs12" style={{ padding: '0 20px 10px' }}>
        Files saved on this device, straight from disk — separate from the Meetings list.
      </p>

      <div className="content content--above-fab">
        {loading && (
          <div className="col ac jc" style={{ padding: '60px 12px' }}>
            <p className="muted fs13">Loading…</p>
          </div>
        )}

        {!loading && recordings.length === 0 && (
          <div className="col ac jc" style={{ padding: '60px 32px', textAlign: 'center', gap: 12 }}>
            <FileAudio size={40} color="hsl(var(--muted-fg, var(--fg)))" strokeWidth={1.5} />
            <h2 style={{ fontSize: 17, fontWeight: 800, margin: 0 }}>No recordings on disk yet</h2>
            <p className="muted fs13" style={{ margin: 0, lineHeight: 1.5, maxWidth: 260 }}>
              Once you record a meeting, its files will show up here.
            </p>
          </div>
        )}

        {recordings.map((rec) => {
          const isOpen = expanded === rec.folderName;
          return (
            <div
              key={rec.folderName}
              style={{ borderBottom: '1px solid hsl(var(--border))' }}
            >
              <div
                className="row gap12"
                style={{ padding: '14px 20px', cursor: 'pointer' }}
                onClick={() => toggleExpand(rec)}
              >
                <div
                  style={{
                    width: 40,
                    height: 40,
                    borderRadius: 10,
                    background: 'hsl(var(--accent))',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    flexShrink: 0,
                  }}
                >
                  <FileAudio size={17} color="hsl(var(--primary))" />
                </div>
                <div className="col f1" style={{ minWidth: 0, gap: 2 }}>
                  <span
                    style={{
                      fontWeight: 700,
                      fontSize: 14.5,
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {rec.title}
                  </span>
                  <span className="muted mono fs11">
                    {formatDateLine(rec.createdAt)}
                    {rec.durationSeconds ? ` · ${formatDuration(rec.durationSeconds)}` : ''}
                    {rec.status === 'recording' ? ' · in progress' : ''}
                  </span>
                </div>
                <div className="row gap6">
                  {rec.hasAudio && (
                    <span className="chip" style={{ fontSize: 11, padding: '3px 8px' }}>
                      audio
                    </span>
                  )}
                  {rec.hasTranscript && (
                    <span className="chip" style={{ fontSize: 11, padding: '3px 8px' }}>
                      transcript
                    </span>
                  )}
                </div>
              </div>

              {isOpen && (
                <div className="col gap10" style={{ padding: '0 20px 18px 72px' }}>
                  {rec.hasAudio && rec.audioPath && (
                    <div className="col gap8">
                      {!audioUrl && !audioError && (
                        <button
                          className="btn btns sm"
                          style={{ alignSelf: 'flex-start' }}
                          onClick={() => playAudio(rec.audioPath!)}
                        >
                          <Play size={14} style={{ marginRight: 6 }} />
                          Play recording
                        </button>
                      )}
                      {audioUrl && (
                        // eslint-disable-next-line jsx-a11y/media-has-caption
                        <audio controls src={audioUrl} style={{ width: '100%' }} />
                      )}
                      {audioError && (
                        <p className="muted fs12" style={{ margin: 0 }}>
                          Couldn&apos;t load audio: {audioError}
                        </p>
                      )}
                    </div>
                  )}
                  {!rec.hasAudio && (
                    <p className="muted fs12" style={{ margin: 0 }}>
                      No audio file was saved for this recording.
                    </p>
                  )}

                  {segmentsLoading && <p className="muted fs12">Loading transcript…</p>}
                  {!segmentsLoading && segments.length > 0 && (
                    <div className="col gap6" style={{ marginTop: 4 }}>
                      {segments.map((seg) => (
                        <div key={seg.id} className="row gap8" style={{ alignItems: 'flex-start' }}>
                          <span className="mono muted fs11" style={{ paddingTop: 1, flexShrink: 0 }}>
                            {formatTs(seg.timestamp)}
                          </span>
                          <span className="fs13" style={{ lineHeight: 1.5 }}>
                            {seg.text}
                          </span>
                        </div>
                      ))}
                    </div>
                  )}
                  {!segmentsLoading && rec.hasTranscript && segments.length === 0 && (
                    <p className="muted fs12" style={{ margin: 0 }}>
                      No transcript segments were captured.
                    </p>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
