'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from '@/components/ui/switch';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import type { RecordingPreferences } from '@/components/RecordingSettings';
import type { Person } from '@/types';
import { usePeople } from '@/hooks/usePeople';
import { formatLastSeen, formatMeetingCount } from '@/lib/people';
import { errorMessage } from '@/lib/errors';

function PersonRow({ person, others, onChanged }: { person: Person; others: Person[]; onChanged: () => Promise<void> }) {
  const [mode, setMode] = useState<'idle' | 'rename' | 'merge'>('idle');
  const [draft, setDraft] = useState(person.name);
  const [renameError, setRenameError] = useState<string | null>(null);
  const [confirmForget, setConfirmForget] = useState(false);

  const rename = async () => {
    try {
      await invoke('api_rename_person', { personId: person.id, name: draft });
      setMode('idle');
      setRenameError(null);
      await onChanged();
    } catch (error) {
      // A name another person already has comes back as a readable message.
      setRenameError(errorMessage(error, 'Failed to rename'));
    }
  };
  const merge = async (into: Person) => {
    try {
      await invoke('api_merge_people', { fromId: person.id, intoId: into.id });
      setMode('idle');
      toast.success(`Merged ${person.name} into ${into.name}`);
      await onChanged();
    } catch (error) {
      toast.error(errorMessage(error, 'Failed to merge'));
    }
  };
  const forget = async () => {
    try {
      await invoke('api_forget_person', { personId: person.id });
      setConfirmForget(false);
      await onChanged();
    } catch (error) {
      toast.error(errorMessage(error, 'Failed to forget'));
    }
  };

  return (
    <li className="space-y-2 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate font-medium">{person.name}</div>
          <div className="text-xs text-gray-500">
            {`${formatMeetingCount(person.meeting_count)} · last seen ${formatLastSeen(person.last_seen)}`}
          </div>
        </div>
        <div className="flex shrink-0 gap-1">
          <Button
            size="sm"
            variant="ghost"
            aria-label={`Rename ${person.name}`}
            onClick={() => {
              setDraft(person.name);
              setRenameError(null);
              setMode(mode === 'rename' ? 'idle' : 'rename');
            }}
          >
            Rename
          </Button>
          {others.length > 0 && (
            <Button size="sm" variant="ghost" aria-label={`Merge ${person.name}`} onClick={() => setMode(mode === 'merge' ? 'idle' : 'merge')}>
              Merge
            </Button>
          )}
          <Button size="sm" variant="ghost" aria-label={`Forget ${person.name}`} onClick={() => setConfirmForget(true)}>
            Forget
          </Button>
        </div>
      </div>
      {mode === 'rename' && (
        <form
          className="space-y-1"
          onSubmit={async (e) => {
            e.preventDefault();
            await rename();
          }}
        >
          <div className="flex gap-2">
            <Input aria-label={`New name for ${person.name}`} value={draft} onChange={(e) => setDraft(e.target.value)} autoFocus />
            <Button type="submit" size="sm">Save</Button>
          </div>
          {renameError && <p role="alert" className="text-xs text-red-600">{renameError}</p>}
        </form>
      )}
      {mode === 'merge' && (
        <div className="space-y-1">
          <div className="text-xs text-gray-600">{`Same person as… (${person.name}'s meetings move there)`}</div>
          {others.map((other) => (
            <button
              key={other.id}
              type="button"
              aria-label={`Merge ${person.name} into ${other.name}`}
              className="block w-full rounded px-2 py-1 text-left text-sm hover:bg-gray-100"
              onClick={() => merge(other)}
            >
              {other.name}
            </button>
          ))}
        </div>
      )}
      <Dialog open={confirmForget} onOpenChange={setConfirmForget}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{`Forget ${person.name}?`}</DialogTitle>
            <DialogDescription>
              Their voice is no longer recognised. Meetings keep the name as plain text.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmForget(false)}>Cancel</Button>
            <Button variant="destructive" onClick={() => forget()}>Forget voice</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </li>
  );
}

export function SpeakerSettings() {
  const [preferences, setPreferences] = useState<RecordingPreferences | null>(null);
  const [saving, setSaving] = useState(false);
  /** 'offer' after turning remembering off, 'confirm' from the button. */
  const [forgetAllDialog, setForgetAllDialog] = useState<'offer' | 'confirm' | null>(null);
  const { people, refresh } = usePeople(true);

  useEffect(() => {
    invoke<RecordingPreferences>('get_recording_preferences')
      .then(setPreferences)
      .catch((error) => console.error('Failed to load recording preferences:', error));
  }, []);

  const setRememberVoices = async (enabled: boolean) => {
    if (!preferences) return;
    const next = { ...preferences, remember_voices: enabled };
    setSaving(true);
    try {
      await invoke('set_recording_preferences', { preferences: next });
      setPreferences(next);
      toast.success(enabled ? 'Voices are remembered across meetings' : 'Voices are no longer remembered');
      if (!enabled && people.length > 0) setForgetAllDialog('offer');
    } catch (error) {
      toast.error(errorMessage(error, 'Failed to save the setting'));
    } finally {
      setSaving(false);
    }
  };

  const forgetAll = async () => {
    try {
      await invoke('api_forget_all_voices');
      setForgetAllDialog(null);
      toast.success('All voices forgotten');
      await refresh();
    } catch (error) {
      toast.error(errorMessage(error, 'Failed to forget voices'));
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h3 className="mb-1 text-lg font-semibold">Speakers</h3>
        <p className="text-sm text-gray-600">
          Voices you name are recognised in your other meetings. Voice data stays on this device.
        </p>
      </div>

      <div className="flex items-center justify-between rounded-lg border p-4">
        <div className="flex-1">
          <div className="font-medium">Remember voices across meetings</div>
          <div className="text-sm text-gray-600">Name a voice once and it is named in other meetings too</div>
        </div>
        <Switch
          aria-label="Remember voices across meetings"
          checked={preferences?.remember_voices ?? true}
          onCheckedChange={(enabled) => void setRememberVoices(enabled)}
          disabled={!preferences || saving}
        />
      </div>

      <div className="rounded-lg border p-4">
        <div className="flex items-center justify-between">
          <div className="font-medium">People</div>
          {people.length > 0 && (
            <Button size="sm" variant="outline" onClick={() => setForgetAllDialog('confirm')}>Forget all voices…</Button>
          )}
        </div>
        {people.length === 0 ? (
          <p className="mt-2 text-sm text-gray-500">No one yet. Name a speaker in a meeting to add them.</p>
        ) : (
          <ul className="divide-y">
            {people.map((person) => (
              <PersonRow key={person.id} person={person} others={people.filter((p) => p.id !== person.id)} onChanged={refresh} />
            ))}
          </ul>
        )}
      </div>

      <Dialog open={forgetAllDialog !== null} onOpenChange={(open) => { if (!open) setForgetAllDialog(null); }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Forget all voices?</DialogTitle>
            <DialogDescription>
              {forgetAllDialog === 'offer' ? 'New meetings no longer match voices. You can also delete the voices already stored. ' : ''}
              Every person is removed and all suggestions are cleared. Meetings keep their speaker names as plain text.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setForgetAllDialog(null)}>
              {forgetAllDialog === 'offer' ? 'Keep them' : 'Cancel'}
            </Button>
            <Button variant="destructive" onClick={() => forgetAll()}>Forget all</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
