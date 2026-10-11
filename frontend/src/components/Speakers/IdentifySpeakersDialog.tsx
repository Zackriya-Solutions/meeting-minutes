'use client';

import { useEffect, useState } from 'react';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { SpeakerCountSelect } from './SpeakerCountSelect';
import { formatMegabytes, useDiarizationModels } from './DiarizationModelSettings';

interface IdentifySpeakersDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  hasSpeakers: boolean;
  onRun: (numSpeakers: number | null) => Promise<void>;
}

export function IdentifySpeakersDialog({ open, onOpenChange, hasSpeakers, onRun }: IdentifySpeakersDialogProps) {
  const [count, setCount] = useState<number | null>(null);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const models = useDiarizationModels();
  const needsDownload = models.status !== null && !models.status.installed;

  useEffect(() => {
    if (open) setError(null);
  }, [open]);

  const run = async () => {
    setError(null);
    setStarting(true);
    try {
      if (needsDownload) await models.download();
      await onRun(count);
      onOpenChange(false);
    } catch (err) {
      // Download and start failures stay in the dialog, with Retry.
      setError(typeof err === 'string' ? err : 'Something went wrong');
    } finally {
      setStarting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Identify speakers</DialogTitle>
          <DialogDescription>
            Detect who is speaking in this recording. Transcript text only changes on lines where the speaker switches mid-line.
            {hasSpeakers && ' Names you set are kept where voices match.'}
          </DialogDescription>
        </DialogHeader>
        <div className="flex items-center justify-between">
          <span className="text-sm text-gray-700">Number of speakers</span>
          <SpeakerCountSelect value={count} onChange={setCount} />
        </div>
        {needsDownload && (
          <div className="space-y-1 text-sm text-gray-600">
            <div>Speaker models ({formatMegabytes(models.status?.total_bytes ?? 0)}) will be downloaded first.</div>
            {models.downloading && <Progress value={models.progress} />}
          </div>
        )}
        {error && <div className="text-sm text-red-600">{error}</div>}
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button onClick={() => void run()} disabled={starting}>
            {starting ? 'Starting…' : error ? 'Retry' : 'Identify speakers'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
