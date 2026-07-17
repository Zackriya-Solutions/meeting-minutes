export type InterviewMarkerKind = 'strong_example' | 'follow_up' | 'doubt' | 'return';

export interface InterviewLiveMarker {
  id: string;
  kind: InterviewMarkerKind;
  seconds: number;
}

export interface InterviewLiveState {
  enabled: boolean;
  targetMinutes: number;
  candidateQuestionsMinutes: number;
  coveredCompetencies: number;
  markers: InterviewLiveMarker[];
}

const DEFAULT_STATE: InterviewLiveState = {
  enabled: false,
  targetMinutes: 60,
  candidateQuestionsMinutes: 10,
  coveredCompetencies: 0,
  markers: [],
};
const keyFor = (meetingId: string) => `memento:interview-live:${meetingId}`;

function normalize(raw: unknown): InterviewLiveState {
  if (!raw || typeof raw !== 'object') return { ...DEFAULT_STATE };
  const value = raw as Partial<InterviewLiveState>;
  return {
    enabled: value.enabled === true,
    targetMinutes: Math.min(240, Math.max(10, Math.floor(Number(value.targetMinutes) || 60))),
    candidateQuestionsMinutes: Math.min(60, Math.max(0, Math.floor(Number(value.candidateQuestionsMinutes) || 10))),
    coveredCompetencies: Math.min(100, Math.max(0, Math.floor(Number(value.coveredCompetencies) || 0))),
    markers: Array.isArray(value.markers)
      ? value.markers.filter((marker): marker is InterviewLiveMarker => Boolean(
          marker && typeof marker.id === 'string' &&
          ['strong_example', 'follow_up', 'doubt', 'return'].includes(marker.kind) &&
          Number.isFinite(marker.seconds),
        )).slice(-100)
      : [],
  };
}

function read(meetingId: string): InterviewLiveState {
  if (typeof window === 'undefined') return { ...DEFAULT_STATE };
  try {
    const raw = window.localStorage.getItem(keyFor(meetingId));
    return raw ? normalize(JSON.parse(raw)) : { ...DEFAULT_STATE };
  } catch { return { ...DEFAULT_STATE }; }
}

function write(meetingId: string, state: InterviewLiveState): InterviewLiveState {
  const value = normalize(state);
  if (typeof window !== 'undefined') window.localStorage.setItem(keyFor(meetingId), JSON.stringify(value));
  return value;
}

export function getInterviewLiveState(meetingId: string | null | undefined): InterviewLiveState {
  return meetingId ? read(meetingId) : { ...DEFAULT_STATE };
}

export function setInterviewLiveEnabled(meetingId: string | null | undefined, enabled: boolean): InterviewLiveState {
  return meetingId ? write(meetingId, { ...read(meetingId), enabled }) : { ...DEFAULT_STATE, enabled };
}

export function cycleInterviewTarget(meetingId: string | null | undefined): InterviewLiveState {
  if (!meetingId) return { ...DEFAULT_STATE };
  const current = read(meetingId);
  const targets = [30, 45, 60, 75, 90];
  const next = targets[(targets.indexOf(current.targetMinutes) + 1) % targets.length] ?? 60;
  return write(meetingId, { ...current, targetMinutes: next });
}

export function changeCoveredCompetencies(meetingId: string | null | undefined, delta: number): InterviewLiveState {
  if (!meetingId) return { ...DEFAULT_STATE };
  const current = read(meetingId);
  return write(meetingId, { ...current, coveredCompetencies: current.coveredCompetencies + delta });
}

export function addInterviewMarker(meetingId: string | null | undefined, kind: InterviewMarkerKind, seconds: number): InterviewLiveState {
  if (!meetingId) return { ...DEFAULT_STATE };
  const current = read(meetingId);
  const at = Math.max(0, Math.floor(seconds));
  if (current.markers.some((marker) => marker.kind === kind && Math.abs(marker.seconds - at) <= 10)) return current;
  return write(meetingId, { ...current, markers: [...current.markers, { id: `${Date.now()}-${kind}`, kind, seconds: at }] });
}

export function migrateInterviewLiveState(fromMeetingId: string | null | undefined, toMeetingId: string | null | undefined): void {
  if (!fromMeetingId || !toMeetingId || fromMeetingId === toMeetingId) return;
  const source = read(fromMeetingId);
  if (!source.enabled && source.markers.length === 0 && source.coveredCompetencies === 0) return;
  const target = read(toMeetingId);
  write(toMeetingId, { ...source, enabled: source.enabled || target.enabled, markers: [...target.markers, ...source.markers], coveredCompetencies: Math.max(source.coveredCompetencies, target.coveredCompetencies) });
  try { window.localStorage.removeItem(keyFor(fromMeetingId)); } catch { /* best effort */ }
}
