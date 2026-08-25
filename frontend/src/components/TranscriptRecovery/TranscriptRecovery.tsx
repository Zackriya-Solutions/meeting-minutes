/**
 * TranscriptRecovery Component
 *
 * Modal dialog for recovering interrupted meetings from IndexedDB.
 * Displays recoverable meetings, allows preview, and enables recovery or deletion.
 */

import React, { useState, useEffect } from 'react';
import { formatDistanceToNow } from 'date-fns';
import { useTranslations } from 'next-intl';
import { useDateFnsLocale } from '@/lib/date-locale';
import { AlertCircle, CheckCircle2, Clock, FileText, Trash2, XCircle } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { MeetingMetadata, StoredTranscript } from '@/services/indexedDBService';
import { cn } from '@/lib/utils';

interface TranscriptRecoveryProps {
  isOpen: boolean;
  onClose: () => void;
  recoverableMeetings: MeetingMetadata[];
  onRecover: (meetingId: string) => Promise<any>;
  onDelete: (meetingId: string) => Promise<void>;
  onLoadPreview: (meetingId: string) => Promise<StoredTranscript[]>;
}

export function TranscriptRecovery({
  isOpen,
  onClose,
  recoverableMeetings,
  onRecover,
  onDelete,
  onLoadPreview,
}: TranscriptRecoveryProps) {
  const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
  const [previewTranscripts, setPreviewTranscripts] = useState<StoredTranscript[]>([]);
  const [isLoadingPreview, setIsLoadingPreview] = useState(false);
  const [isRecovering, setIsRecovering] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const t = useTranslations('settings');
  const dateFnsLocale = useDateFnsLocale();


  // Reset selection when dialog opens
  useEffect(() => {
    if (isOpen) {
      setSelectedMeetingId(null);
      setPreviewTranscripts([]);
    }
  }, [isOpen]);

  // Auto-select first meeting if available
  useEffect(() => {
    if (isOpen && recoverableMeetings.length > 0 && !selectedMeetingId) {
      handleMeetingSelect(recoverableMeetings[0].meetingId);
    }
  }, [isOpen, recoverableMeetings]);

  const handleMeetingSelect = async (meetingId: string) => {
    setSelectedMeetingId(meetingId);
    setIsLoadingPreview(true);

    try {
      const transcripts = await onLoadPreview(meetingId);
      // Limit to first 10 for preview
      setPreviewTranscripts(transcripts.slice(0, 10));
    } catch (error) {
      console.error('Failed to load preview:', error);
      setPreviewTranscripts([]);
    } finally {
      setIsLoadingPreview(false);
    }
  };

  const handleRecover = async () => {
    if (!selectedMeetingId) return;

    setIsRecovering(true);
    try {
      const result = await onRecover(selectedMeetingId);
      console.log('Recovery successful:', result);
      onClose();
    } catch (error) {
      console.error('Recovery failed:', error);
      alert(t('recovery.alert_recover_failed'));
    } finally {
      setIsRecovering(false);
    }
  };

  const handleDelete = async () => {
    if (!selectedMeetingId) return;

    if (!confirm(t('recovery.confirm_delete'))) {
      return;
    }

    setIsDeleting(true);
    try {
      await onDelete(selectedMeetingId);
      setSelectedMeetingId(null);
      setPreviewTranscripts([]);
    } catch (error) {
      console.error('Delete failed:', error);
      alert(t('recovery.alert_delete_failed'));
    } finally {
      setIsDeleting(false);
    }
  };

  const selectedMeeting = recoverableMeetings.find(m => m.meetingId === selectedMeetingId);

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-4xl h-[80vh] flex flex-col p-0">
        <DialogHeader className="px-6 pt-6">
          <DialogTitle className="text-2xl">{t("recovery.dialog_title")}</DialogTitle>
          <DialogDescription>
            {recoverableMeetings.length === 1 ? t('recovery.dialog_description_one') : t('recovery.dialog_description_other', { count: recoverableMeetings.length })}
          </DialogDescription>
        </DialogHeader>

        <div className="flex-1 flex gap-4 px-6 pb-6 overflow-hidden">
          {/* Meeting List */}
          <div className="w-1/3 flex flex-col">
            <h3 className="text-sm font-medium mb-2">{t("recovery.interrupted_label")}</h3>
            <ScrollArea className="flex-1 border rounded-lg">
              <div className="p-2 space-y-2">
                {recoverableMeetings.map((meeting) => (
                  <button
                    key={meeting.meetingId}
                    onClick={() => handleMeetingSelect(meeting.meetingId)}
                    className={cn(
                      'w-full text-left p-3 rounded-lg border transition-colors',
                      selectedMeetingId === meeting.meetingId
                        ? 'bg-primary/10 border-primary'
                        : 'hover:bg-muted border-transparent'
                    )}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div className="flex-1 min-w-0">
                        <p className="font-medium text-sm truncate">{meeting.title}</p>
                        <p className="text-xs text-muted-foreground flex items-center gap-1 mt-1">
                          <Clock className="w-3 h-3" />
                          {formatDistanceToNow(new Date(meeting.lastUpdated), { addSuffix: true, locale: dateFnsLocale })}
                        </p>
                        <p className="text-xs text-muted-foreground flex items-center gap-1 mt-1">
                          <FileText className="w-3 h-3" />
                          {meeting.transcriptCount} transcript{meeting.transcriptCount !== 1 ? 's' : ''}
                        </p>
                      </div>
                      {meeting.folderPath ? (
                        <span title={t("recovery.audio_available")}>
                          <CheckCircle2 className="w-4 h-4 text-green-500 flex-shrink-0" />
                        </span>
                      ) : (
                        <span title={t("recovery.audio_unavailable")}>
                          <AlertCircle className="w-4 h-4 text-yellow-500 flex-shrink-0" />
                        </span>
                      )}
                    </div>
                  </button>
                ))}
              </div>
            </ScrollArea>
          </div>

          {/* Preview Panel */}
          <div className="flex-1 flex flex-col">
            <h3 className="text-sm font-medium mb-2">{t("recovery.preview_label")}</h3>
            <div className="flex-1 border rounded-lg overflow-hidden flex flex-col">
              {selectedMeeting ? (
                <>
                  {/* Meeting Info */}
                  <div className="p-4 border-b bg-muted/50">
                    <h4 className="font-semibold">{selectedMeeting.title}</h4>
                    <p className="text-sm text-muted-foreground mt-1">
                      {t('recovery.started_label')} {new Date(selectedMeeting.startTime).toLocaleString()}
                    </p>
                    <div className="flex items-center gap-4 mt-2 text-sm">
                      <span className="flex items-center gap-1">
                        <FileText className="w-4 h-4" />
                        {selectedMeeting.transcriptCount} transcripts
                      </span>
                      {selectedMeeting.folderPath ? (
                        <span className="flex items-center gap-1 text-green-600">
                          <CheckCircle2 className="w-4 h-4" />
                          {t('recovery.audio_available')}
                        </span>
                      ) : (
                        <span className="flex items-center gap-1 text-yellow-600">
                          <AlertCircle className="w-4 h-4" />
                          {t('recovery.audio_unavailable')}
                        </span>
                      )}
                    </div>
                  </div>

                  {/* Transcript Preview */}
                  <ScrollArea className="flex-1 p-4">
                    {isLoadingPreview ? (
                      <div className="flex items-center justify-center h-full text-muted-foreground">
                        {t('recovery.preview_loading')}
                      </div>
                    ) : previewTranscripts.length > 0 ? (
                      <div className="space-y-3">
                        <Alert>
                          <AlertDescription>
                            {t('recovery.preview_segments', { shown: previewTranscripts.length, total: selectedMeeting.transcriptCount })}
                          </AlertDescription>
                        </Alert>
                        {previewTranscripts.map((transcript, index) => {
                          // Handle different timestamp formats
                          const getTimestamp = () => {
                            if (!transcript.timestamp) return '--:--';
                            try {
                              const date = new Date(transcript.timestamp);
                              if (isNaN(date.getTime())) {
                                // If timestamp is invalid, try audio_start_time
                                if (transcript.audio_start_time !== undefined) {
                                  const totalSecs = Math.floor(transcript.audio_start_time);
                                  const mins = Math.floor(totalSecs / 60);
                                  const secs = totalSecs % 60;
                                  return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
                                }
                                return '--:--';
                              }
                              return date.toLocaleTimeString();
                            } catch {
                              return '--:--';
                            }
                          };

                          return (
                            <div key={index} className="text-sm">
                              <span className="text-muted-foreground">[{getTimestamp()}]</span>{' '}
                              <span>{transcript.text}</span>
                            </div>
                          );
                        })}
                        {selectedMeeting.transcriptCount > 10 && (
                          <p className="text-sm text-muted-foreground italic">
                            ... and {selectedMeeting.transcriptCount - 10} more transcript{selectedMeeting.transcriptCount - 10 !== 1 ? 's' : ''}
                          </p>
                        )}
                      </div>
                    ) : (
                      <div className="flex items-center justify-center h-full text-muted-foreground">
                        {t('recovery.preview_empty')}
                      </div>
                    )}
                  </ScrollArea>
                </>
              ) : (
                <div className="flex items-center justify-center h-full text-muted-foreground">
                  {t('recovery.preview_select')}
                </div>
              )}
            </div>
          </div>
        </div>

        <DialogFooter className="px-6 pb-6">
          <Button
            variant="outline"
            onClick={onClose}
            disabled={isRecovering || isDeleting}
          >
            {t('recovery.cancel')}
          </Button>
          <Button
            variant="destructive"
            onClick={handleDelete}
            disabled={!selectedMeetingId || isRecovering || isDeleting}
          >
            {isDeleting ? (
              <>
                <XCircle className="w-4 h-4 mr-2 animate-spin" />
                {t('recovery.deleting')}
              </>
            ) : (
              <>
                <Trash2 className="w-4 h-4 mr-2" />
                {t('recovery.delete')}
              </>
            )}
          </Button>
          <Button
            onClick={handleRecover}
            disabled={!selectedMeetingId || isRecovering || isDeleting}
          >
            {isRecovering ? (
              <>
                <CheckCircle2 className="w-4 h-4 mr-2 animate-spin" />
                {t('recovery.recovering')}
              </>
            ) : (
              <>
                <CheckCircle2 className="w-4 h-4 mr-2" />
                {t('recovery.recover')}
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
