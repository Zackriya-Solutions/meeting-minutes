"use client";

import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, FolderOpen, Music } from 'lucide-react';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';

interface TranscriptButtonGroupProps {
  transcriptCount: number;
  onCopyTranscript: () => void;
}

export function TranscriptButtonGroup({
  transcriptCount,
  onCopyTranscript
}: TranscriptButtonGroupProps) {
  const handleOpenFolder = async () => {
    try {
      Analytics.trackButtonClick('open_recording_folder', 'meeting_details');
      await invoke('open_recordings_folder');
    } catch (error) {
      console.error('Failed to open recording folder:', error);
    }
  };

  return (
    <div className="flex items-center justify-center w-full gap-2">
      <ButtonGroup>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            Analytics.trackButtonClick('copy_transcript', 'meeting_details');
            onCopyTranscript();
          }}
          disabled={transcriptCount === 0}
          title={transcriptCount === 0 ? 'No transcript available' : 'Copy Transcript'}
        >
          <Copy />
          <span className="hidden lg:inline">Copy</span>
        </Button>

        <Button
          variant="outline"
          size="sm"
          onClick={handleOpenFolder}
          title="Open Recording Folder"
        >
          <Music />
          <span className="hidden lg:inline">Recording</span>
        </Button>
      </ButtonGroup>
    </div>
  );
}
