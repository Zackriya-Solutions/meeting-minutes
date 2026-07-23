const SPEAKER_COLORS = [
  '#2563eb',
  '#7c3aed',
  '#059669',
  '#d97706',
  '#dc2626',
  '#0891b2',
  '#c026d3',
  '#4f46e5',
  '#65a30d',
  '#ea580c',
];

export function formatSpeakerLabel(speaker?: string | null): string | null {
  if (!speaker) return null;
  const match = /^speaker_(\d+)$/.exec(speaker);
  if (!match) return speaker;
  return `Speaker ${Number.parseInt(match[1], 10) + 1}`;
}

export function speakerColor(speaker?: string | null): string {
  if (!speaker) return '#6b7280';
  const match = /^speaker_(\d+)$/.exec(speaker);
  const index = match
    ? Number.parseInt(match[1], 10)
    : Array.from(speaker).reduce((sum, char) => sum + char.charCodeAt(0), 0);
  return SPEAKER_COLORS[index % SPEAKER_COLORS.length];
}

export function prefixSpeaker(text: string, speaker?: string | null): string {
  const label = formatSpeakerLabel(speaker);
  return label ? `${label}: ${text}` : text;
}
