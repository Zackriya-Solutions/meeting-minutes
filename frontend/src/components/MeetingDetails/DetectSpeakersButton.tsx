"use client";

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { Users, Loader2, Download } from '@/components/memento/LucideCompat';
import { Button } from "../ui/button";
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "../ui/dialog";
import { DiarizationStatus, DiarizeMeetingResult } from "@/types";
import Analytics from "@/lib/analytics";

interface DetectSpeakersButtonProps {
    meetingId?: string;
    /** Called after a successful diarization so the caller can refresh speakers + transcripts. */
    onDetected?: () => Promise<void> | void;
}

// Idle → user can start. checking → confirming models exist. diarizing → running.
type Phase = "idle" | "checking" | "diarizing";

const errString = (err: unknown, fallback: string): string =>
    typeof err === "string" ? err : (err as any)?.message || fallback;

/**
 * "Detect speakers" action for a saved meeting. Verifies the diarization models
 * exist (offering a one-time ~35 MB download if not), runs `diarize_meeting`,
 * and reports the outcome via toast — matching the retranscription idiom.
 */
export function DetectSpeakersButton({ meetingId, onDetected }: DetectSpeakersButtonProps) {
    const [phase, setPhase] = useState<Phase>("idle");
    const [showDownload, setShowDownload] = useState(false);
    const [downloading, setDownloading] = useState(false);

    const busy = phase !== "idle";

    const runDiarize = async () => {
        if (!meetingId) return;
        setPhase("diarizing");
        try {
            const result = await invoke<DiarizeMeetingResult>("diarize_meeting", { meetingId });
            const plural = result.speaker_count === 1 ? "speaker" : "speakers";
            toast.success(
                `Found ${result.speaker_count} ${plural} · ${result.assigned_segments}/${result.total_segments} segments attributed`
            );
            await onDetected?.();
        } catch (err) {
            toast.error(errString(err, "Speaker detection failed"));
        } finally {
            setPhase("idle");
        }
    };

    const handleClick = async () => {
        if (!meetingId || busy) return;
        Analytics.trackButtonClick("detect_speakers", "meeting_details");
        setPhase("checking");
        try {
            const status = await invoke<DiarizationStatus>("diarization_status");
            if (!status.available) {
                setPhase("idle");
                setShowDownload(true);
                return;
            }
            await runDiarize();
        } catch (err) {
            setPhase("idle");
            toast.error(errString(err, "Could not check speaker models"));
        }
    };

    const handleDownloadAndDetect = async () => {
        setDownloading(true);
        try {
            await invoke("download_diarization_models");
            setDownloading(false);
            setShowDownload(false);
            await runDiarize();
        } catch (err) {
            setDownloading(false);
            toast.error(errString(err, "Failed to download speaker models"));
        }
    };

    if (!meetingId) return null;

    return (
        <>
            <Button
                size="sm"
                variant="outline"
                className="xl:px-4"
                onClick={handleClick}
                disabled={busy}
                title="Detect and label speakers in this meeting"
            >
                {busy ? (
                    <Loader2 className="xl:mr-2 animate-spin" size={18} />
                ) : (
                    <Users className="xl:mr-2" size={18} />
                )}
                <span className="hidden lg:inline">
                    {phase === "diarizing" ? "Detecting..." : "Speakers"}
                </span>
            </Button>

            <Dialog
                open={showDownload}
                onOpenChange={(o) => { if (!downloading) setShowDownload(o); }}
            >
                <DialogContent className="sm:max-w-[420px]">
                    <DialogHeader>
                        <DialogTitle className="flex items-center gap-2">
                            <Users className="h-5 w-5 text-[var(--gold)]" />
                            Detect Speakers
                        </DialogTitle>
                        <DialogDescription>
                            Speaker detection needs a one-time download of the diarization models
                            (~35 MB). They are stored locally and reused for future meetings.
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            variant="outline"
                            onClick={() => setShowDownload(false)}
                            disabled={downloading}
                        >
                            Cancel
                        </Button>
                        <Button
                            onClick={handleDownloadAndDetect}
                            disabled={downloading}
                            className="bg-[var(--gold)] hover:bg-[var(--gold-active)]"
                        >
                            {downloading ? (
                                <>
                                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                                    Downloading...
                                </>
                            ) : (
                                <>
                                    <Download className="h-4 w-4 mr-2" />
                                    Download &amp; Detect
                                </>
                            )}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    );
}
