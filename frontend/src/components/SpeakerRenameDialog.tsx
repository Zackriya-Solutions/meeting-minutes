'use client';

// Rename a detected speaker ("Speaker 1" → "Alice") across all segments of a
// meeting. Optionally remembers the voice as a persistent profile so future
// recordings label this speaker by name automatically.

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import { toast } from 'sonner';

interface SpeakerRenameDialogProps {
  meetingId: string;
  speakerLabel: string;
  onClose: () => void;
  onRenamed: () => void | Promise<void>;
}

interface RenameResult {
  updated_segments: number;
  profile_saved: boolean;
}

export function SpeakerRenameDialog({
  meetingId,
  speakerLabel,
  onClose,
  onRenamed,
}: SpeakerRenameDialogProps) {
  const [name, setName] = useState('');
  const [saveProfile, setSaveProfile] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  const handleRename = async () => {
    if (!name.trim()) return;
    setIsSaving(true);
    try {
      const result = await invoke<RenameResult>('diarization_rename_speaker', {
        meetingId,
        oldLabel: speakerLabel,
        newName: name.trim(),
        saveProfile,
      });
      toast.success(
        `Renamed ${speakerLabel} to ${name.trim()} (${result.updated_segments} segments)` +
          (result.profile_saved ? ' — voice remembered' : '')
      );
      if (saveProfile && !result.profile_saved) {
        toast.warning(
          'Voice data was not available for this meeting, so the profile was not saved.'
        );
      }
      await onRenamed();
    } catch (err) {
      console.error('Failed to rename speaker:', err);
      toast.error(`Failed to rename speaker: ${err}`);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Rename {speakerLabel}</DialogTitle>
          <DialogDescription>
            The new name is applied to every segment from this speaker in this meeting.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 py-2">
          <div>
            <Label htmlFor="speaker-name" className="text-sm">
              Name
            </Label>
            <Input
              id="speaker-name"
              autoFocus
              value={name}
              placeholder="e.g. Alice"
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleRename()}
            />
          </div>
          <label className="flex items-center gap-2 text-sm text-gray-600 cursor-pointer">
            <input
              type="checkbox"
              checked={saveProfile}
              onChange={(e) => setSaveProfile(e.target.checked)}
              className="rounded border-gray-300"
            />
            Remember this voice for future meetings (stored only on this device)
          </label>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={isSaving}>
            Cancel
          </Button>
          <Button onClick={handleRename} disabled={!name.trim() || isSaving}>
            {isSaving ? 'Renaming…' : 'Rename'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
