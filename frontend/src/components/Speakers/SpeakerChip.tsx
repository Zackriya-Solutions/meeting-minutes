'use client';

import { memo, useState } from 'react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { MeetingSpeaker } from '@/types';
import { speakerColor, speakerLabel, speakerNameState } from '@/lib/speakers';
import { attempt } from '@/lib/errors';
import { SpeakerRenameForm } from './SpeakerRenameForm';
import { SpeakerNameBadge } from './SpeakerNameBadge';

interface SpeakerChipProps {
  speakerKey: string;
  transcriptId: string;
  speakers: MeetingSpeaker[];
  names: Record<string, string>;
  editable: boolean;
  /** Small dot trigger for rows inside a same-speaker run. */
  compact?: boolean;
  onRename: (key: string, name: string) => Promise<void>;
  onMerge: (fromKey: string, intoKey: string) => Promise<void>;
  onReassign: (transcriptId: string, key: string | null) => Promise<void>;
  onConfirm: (key: string) => Promise<void>;
  onReject: (key: string) => Promise<void>;
  /** Plays a sample of the speaker; only full chips show the button. Stable. */
  onPlaySample?: (key: string) => void;
}

function SpeakerOptionButtons({ speakers, names, ariaPrefix, onPick }: {
  speakers: MeetingSpeaker[];
  names: Record<string, string>;
  ariaPrefix: string;
  onPick: (key: string) => void;
}) {
  return (
    <>
      {speakers.map((s) => {
        const otherLabel = speakerLabel(s.speaker_key, names);
        return (
          <button
            key={s.speaker_key}
            type="button"
            aria-label={`${ariaPrefix} ${otherLabel}`}
            className="block w-full rounded px-2 py-1 text-left text-sm hover:bg-gray-100"
            onClick={() => onPick(s.speaker_key)}
          >
            {otherLabel}
          </button>
        );
      })}
    </>
  );
}

function SpeakerChipImpl({
  speakerKey,
  transcriptId,
  speakers,
  names,
  editable,
  compact = false,
  onRename,
  onMerge,
  onReassign,
  onConfirm,
  onReject,
  onPlaySample,
}: SpeakerChipProps) {
  const [open, setOpen] = useState(false);
  const label = speakerLabel(speakerKey, names);
  const color = speakerColor(speakerKey);
  const current = speakers.find((s) => s.speaker_key === speakerKey);
  const run = async (action: () => Promise<void>, failure: string) => {
    if (await attempt(action, failure)) setOpen(false);
  };
  const chip = (
    <span className={`inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-medium ${color.chip}`}>{label}</span>
  );
  const badge = (
    <SpeakerNameBadge
      state={speakerNameState(current)}
      label={label}
      editable={editable}
      onConfirm={() => void run(() => onConfirm(speakerKey), 'Failed to confirm the name')}
      onReject={() => void run(() => onReject(speakerKey), 'Failed to reject the name')}
      onPlaySample={onPlaySample ? () => onPlaySample(speakerKey) : undefined}
    />
  );
  if (!editable) return compact ? chip : <span className="inline-flex items-center gap-1">{chip}{badge}</span>;

  // Speakers left without rows (every line reassigned away) are not offered.
  const others = speakers.filter((s) => s.speaker_key !== speakerKey && s.row_count > 0);
  const popover = (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        {compact ? (
          <button
            type="button"
            aria-label={`Change speaker (${label})`}
            title="Change speaker"
            className={`h-3 w-3 rounded-full ${color.dot}`}
          />
        ) : (
          <button type="button" className="cursor-pointer" title="Edit speaker">{chip}</button>
        )}
      </PopoverTrigger>
      <PopoverContent className="w-64 space-y-3" align="start">
        <SpeakerRenameForm
          speakerKey={speakerKey}
          initialName={current?.display_name ?? ''}
          placeholder={label}
          label={`Name for everyone labelled ${label}`}
          onRename={onRename}
          onSaved={() => setOpen(false)}
        />
        {others.length > 0 && (
          <div className="space-y-1">
            <div className="text-xs font-medium text-gray-600">Same person as…</div>
            <SpeakerOptionButtons
              speakers={others}
              names={names}
              ariaPrefix="Same person as"
              onPick={(key) => void run(() => onMerge(speakerKey, key), 'Failed to merge speakers')}
            />
          </div>
        )}
        <div className="space-y-1">
          <div className="text-xs font-medium text-gray-600">This line was said by…</div>
          <SpeakerOptionButtons
            speakers={others}
            names={names}
            ariaPrefix="This line was said by"
            onPick={(key) => void run(() => onReassign(transcriptId, key), 'Failed to change speaker')}
          />
          <button
            type="button"
            aria-label="This line was said by a new speaker"
            className="block w-full rounded px-2 py-1 text-left text-sm text-gray-600 hover:bg-gray-100"
            onClick={() => void run(() => onReassign(transcriptId, null), 'Failed to change speaker')}
          >
            New speaker
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
  // The badge stays outside the trigger: its buttons cannot sit inside the chip's button.
  return compact ? popover : <span className="inline-flex items-center gap-1">{popover}{badge}</span>;
}

/** Memoised: every prop is a primitive or a stable reference, so scrolling does not re-render it. */
export const SpeakerChip = memo(SpeakerChipImpl);
