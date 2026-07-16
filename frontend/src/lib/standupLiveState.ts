export type StandupLiveMarkerKind = 'parking_lot' | 'question';

export interface StandupLiveMarker {
  id: string;
  kind: StandupLiveMarkerKind;
  seconds: number;
}

export interface StandupLiveState {
  enabled: boolean;
  targetMinutes: number;
  completedUpdates: number;
  markers: StandupLiveMarker[];
}

const DEFAULT_STATE: StandupLiveState = {
  enabled: false,
  targetMinutes: 15,
  completedUpdates: 0,
  markers: [],
};
const TARGETS = [10, 15, 20, 30];
const MAX_MARKERS = 100;
const MARKER_DEDUPE_WINDOW_SECONDS = 3;
const keyFor = (meetingId: string) => `memento:standup-live:${meetingId}`;

function normalize(raw: unknown): StandupLiveState {
  if (!raw || typeof raw !== 'object') return { ...DEFAULT_STATE };
  const value = raw as Partial<StandupLiveState>;
  const targetMinutes = TARGETS.includes(Number(value.targetMinutes))
    ? Number(value.targetMinutes)
    : DEFAULT_STATE.targetMinutes;
  const completedUpdates = Math.min(
    100,
    Math.max(0, Math.floor(Number(value.completedUpdates) || 0)),
  );
  const markers = Array.isArray(value.markers)
    ? value.markers
        .filter((marker): marker is StandupLiveMarker => {
          if (!marker || typeof marker !== 'object') return false;
          return (
            typeof marker.id === 'string' &&
            (marker.kind === 'parking_lot' || marker.kind === 'question') &&
            Number.isFinite(marker.seconds) &&
            marker.seconds >= 0
          );
        })
        .slice(-MAX_MARKERS)
        .map((marker) => ({ ...marker, seconds: Math.floor(marker.seconds) }))
    : [];
  return {
    enabled: value.enabled === true,
    targetMinutes,
    completedUpdates,
    markers,
  };
}

function read(meetingId: string): StandupLiveState {
  if (typeof window === 'undefined' || !meetingId) return { ...DEFAULT_STATE };
  try {
    const raw = window.localStorage.getItem(keyFor(meetingId));
    return raw ? normalize(JSON.parse(raw)) : { ...DEFAULT_STATE };
  } catch {
    return { ...DEFAULT_STATE };
  }
}

function write(meetingId: string, state: StandupLiveState): StandupLiveState {
  const normalized = normalize(state);
  if (typeof window !== 'undefined' && meetingId) {
    try {
      window.localStorage.setItem(keyFor(meetingId), JSON.stringify(normalized));
    } catch (error) {
      console.warn('Failed to persist live standup state:', error);
    }
  }
  return normalized;
}

export function getStandupLiveState(
  meetingId: string | null | undefined,
): StandupLiveState {
  return meetingId ? read(meetingId) : { ...DEFAULT_STATE };
}

export function setStandupLiveEnabled(
  meetingId: string | null | undefined,
  enabled: boolean,
): StandupLiveState {
  if (!meetingId) return { ...DEFAULT_STATE, enabled };
  return write(meetingId, { ...read(meetingId), enabled });
}

export function setStandupTargetMinutes(
  meetingId: string | null | undefined,
  targetMinutes: number,
): StandupLiveState {
  if (!meetingId) return { ...DEFAULT_STATE };
  return write(meetingId, { ...read(meetingId), targetMinutes });
}

export function changeCompletedStandupUpdates(
  meetingId: string | null | undefined,
  delta: number,
): StandupLiveState {
  if (!meetingId) return { ...DEFAULT_STATE };
  const current = read(meetingId);
  return write(meetingId, {
    ...current,
    completedUpdates: current.completedUpdates + delta,
  });
}

export function addStandupLiveMarker(
  meetingId: string | null | undefined,
  kind: StandupLiveMarkerKind,
  seconds: number,
): StandupLiveState {
  if (!meetingId) return { ...DEFAULT_STATE };
  const current = read(meetingId);
  const normalizedSeconds = Math.max(0, Math.floor(seconds));
  const duplicate = current.markers.some(
    (marker) =>
      marker.kind === kind &&
      Math.abs(marker.seconds - normalizedSeconds) <= MARKER_DEDUPE_WINDOW_SECONDS,
  );
  if (duplicate) return current;
  return write(meetingId, {
    ...current,
    markers: [
      ...current.markers,
      {
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        kind,
        seconds: normalizedSeconds,
      },
    ],
  });
}

export function migrateStandupLiveState(
  fromMeetingId: string | null | undefined,
  toMeetingId: string | null | undefined,
): void {
  if (!fromMeetingId || !toMeetingId || fromMeetingId === toMeetingId) return;
  const source = read(fromMeetingId);
  const target = read(toMeetingId);
  const hasSourceActivity =
    source.enabled || source.completedUpdates > 0 || source.markers.length > 0;
  if (!hasSourceActivity) return;
  write(toMeetingId, {
    enabled: source.enabled || target.enabled,
    targetMinutes: source.targetMinutes,
    completedUpdates: Math.max(source.completedUpdates, target.completedUpdates),
    markers: [...target.markers, ...source.markers],
  });
  try {
    window.localStorage.removeItem(keyFor(fromMeetingId));
  } catch {
    // The saved meeting still has the merged state; source cleanup is best effort.
  }
}

export function clearStandupLiveState(meetingId: string | null | undefined): void {
  if (!meetingId) return;
  try {
    window.localStorage.removeItem(keyFor(meetingId));
  } catch {
    // Cleanup is best effort; recording shutdown must not fail on storage access.
  }
}
