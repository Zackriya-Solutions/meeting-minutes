import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/**
 * Report sections shown inside the meeting screen: the score with its verdict,
 * «Что мешало» and «Покрытие повестки» under the summary, plus the «Числа встречи» and
 * «Динамика встречи» tabs.
 *
 * The numbers are not recomputed here — they are read back from the artifacts snapshot of
 * the meeting's latest COMPLETED analytical report (`get_meeting_analytics_sections`), so
 * what the screen shows and what the exported HTML report shows are the same run. `sections`
 * stays null until a report exists; the tabs then offer to build one.
 *
 * Refreshes itself when a report finishes for this meeting, so the sections appear as soon
 * as a build started from the "⋯" menu (or from a tab's own button) completes.
 */

export interface AnalyticsScoreSection {
  total: number;
  /** Synthesis one-liner; empty string when that stage produced none. */
  verdict: string;
  coverage_pct: number;
  owners_pct: number;
  deadline_pct: number;
  dod_pct: number;
  qa_pct: number;
}

/** `at_seconds` is null when the moment could not be resolved (see the Rust side). */
export interface AnalyticsAgendaRow {
  item: string;
  /** "covered" | "partial" | "missed" */
  status: string;
  at_seconds: number | null;
}

export interface AnalyticsNumberRow {
  metric: string;
  value: string;
  check: string;
  /** "ok" | "warn" | "info" */
  status: string;
  at_seconds: number | null;
}

export interface AnalyticsSpeakerRow {
  label: string;
  talk_secs: number;
  /** Share of total speech time (0..1). */
  talk_share: number;
  questions: number;
  turns: number;
}

export interface AnalyticsDynamicsSection {
  duration_secs: number;
  /** Fraction of wall-clock time that was speech (0..1). */
  speech_density: number;
  turn_count: number;
  total_questions: number;
  pauses_over_3s: number;
  pauses_over_10s: number;
  /** null when the stage failed — the tile shows "—" instead of a wrong zero. */
  decisions_count: number | null;
  commitments_count: number | null;
  speakers: AnalyticsSpeakerRow[];
}

export interface AnalyticsRoleRow {
  speaker: string;
  role: string;
  evidence: string;
  at_seconds: number | null;
}

/** One speaker lane of «Лента встречи»; a turn's `lane` indexes into the lane list. */
export interface AnalyticsTimelineLane {
  label: string;
  /** Colour slot 0..=3; 4+ share the muted slot, as in the HTML report. */
  palette_index: number;
}

export interface AnalyticsTimelineTurn {
  start: number;
  end: number;
  lane: number;
  text: string;
}

export interface AnalyticsTimelineTopic {
  start: number;
  end: number;
  name: string;
}

export interface AnalyticsTimelineMarker {
  at_seconds: number;
  /** "decision" | "disagreement" | "commitment" */
  kind: string;
  text: string;
}

export interface AnalyticsTimelineSection {
  duration_secs: number;
  lanes: AnalyticsTimelineLane[];
  turns: AnalyticsTimelineTurn[];
  topics: AnalyticsTimelineTopic[];
  markers: AnalyticsTimelineMarker[];
}

export interface MeetingAnalyticsSections {
  report_id: string;
  completed_at: string | null;
  score: AnalyticsScoreSection | null;
  what_hindered: string[];
  agenda: AnalyticsAgendaRow[];
  numbers: AnalyticsNumberRow[];
  dynamics: AnalyticsDynamicsSection | null;
  roles: AnalyticsRoleRow[];
  /** null when the transcript could not be read — the feed then has nothing to draw. */
  timeline: AnalyticsTimelineSection | null;
}

interface AnalyticsCompleteEvent {
  meeting_id: string;
}

export interface UseMeetingAnalyticsSectionsResult {
  sections: MeetingAnalyticsSections | null;
  /** True only for the first load of a meeting, so tabs don't flicker on refresh. */
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

export function useMeetingAnalyticsSections(
  meetingId: string | null,
): UseMeetingAnalyticsSectionsResult {
  const [sections, setSections] = useState<MeetingAnalyticsSections | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!meetingId) return;
    try {
      const result = await invoke<MeetingAnalyticsSections | null>(
        'get_meeting_analytics_sections',
        { meetingId },
      );
      setSections(result ?? null);
      setError(null);
    } catch (e) {
      console.error('Failed to load meeting analytics sections:', e);
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [meetingId]);

  // Drop the previous meeting's sections before fetching so they never flash on the new one.
  useEffect(() => {
    setSections(null);
    setError(null);
    if (!meetingId) return;
    let active = true;
    setLoading(true);
    void (async () => {
      try {
        const result = await invoke<MeetingAnalyticsSections | null>(
          'get_meeting_analytics_sections',
          { meetingId },
        );
        if (!active) return;
        setSections(result ?? null);
      } catch (e) {
        console.error('Failed to load meeting analytics sections:', e);
        if (active) setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (active) setLoading(false);
      }
    })();
    return () => { active = false; };
  }, [meetingId]);

  useEffect(() => {
    if (!meetingId) return;
    let unlisten: (() => void) | undefined;
    void (async () => {
      unlisten = await listen<AnalyticsCompleteEvent>('analytics-report-complete', (event) => {
        if (event.payload?.meeting_id !== meetingId) return;
        void load();
      });
    })();
    return () => { unlisten?.(); };
  }, [meetingId, load]);

  return { sections, loading, error, refresh: load };
}
