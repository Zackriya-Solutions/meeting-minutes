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
import { DiarizationStatus, DiarizeMeetingResult, SpeakerInfo } from "@/types";
import Analytics from "@/lib/analytics";
import { useT } from "@/lib/i18n";

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
    const t = useT();
    const [phase, setPhase] = useState<Phase>("idle");
    const [showDownload, setShowDownload] = useState(false);
    const [showRerunConfirmation, setShowRerunConfirmation] = useState(false);
    const [existingSpeakerCount, setExistingSpeakerCount] = useState(0);
    const [downloading, setDownloading] = useState(false);

    const busy = phase !== "idle";

    const runDiarize = async () => {
        if (!meetingId) return;
        setPhase("diarizing");
        try {
            const result = await invoke<DiarizeMeetingResult>("diarize_meeting", { meetingId });
            const plural = result.speaker_count === 1 ? t('speaker') : t('speakers');
            toast.success(
                `${t('Found')} ${result.speaker_count} ${plural} · ${result.assigned_segments}/${result.total_segments} ${t('segments attributed')}`
            );
            await onDetected?.();
        } catch (err) {
            const message = errString(err, t("Speaker detection failed"));
            if (message.includes("local diarization models are not downloaded")) {
                setShowDownload(true);
                toast.warning(t("SaluteSpeech is unavailable. Download the local speaker models to continue without the cloud."));
            } else {
                toast.error(message);
            }
        } finally {
            setPhase("idle");
        }
    };

    const checkModelsAndRun = async () => {
        if (!meetingId || busy) return;
        setPhase("checking");
        try {
            // Cloud diarization (SaluteSpeech) skips the local-model download gate — it
            // works through the managed Memento gateway (or a user Authorization Key).
            // Default to SaluteSpeech when nothing is persisted (managed build default).
            let provider = "salutespeech";
            let saluteReady = false;
            try {
                const s = await invoke<Record<string, string>>("get_app_settings");
                provider = (s?.["diarization.provider"] || "salutespeech").trim() || "salutespeech";
            } catch {
                /* settings unreadable → fall back to the salutespeech default */
            }
            try {
                saluteReady = await invoke<boolean>("salutespeech_is_configured");
            } catch {
                saluteReady = false;
            }

            if (provider === "salutespeech") {
                if (!saluteReady) {
                    const localStatus = await invoke<DiarizationStatus>("diarization_status");
                    if (!localStatus.available) {
                        setPhase("idle");
                        setShowDownload(true);
                        toast.warning(t("SaluteSpeech is unavailable. Download the local speaker models to continue without the cloud."));
                        return;
                    }
                }
                await runDiarize();
                return;
            }

            const status = await invoke<DiarizationStatus>("diarization_status");
            if (!status.available) {
                setPhase("idle");
                setShowDownload(true);
                return;
            }
            await runDiarize();
        } catch (err) {
            setPhase("idle");
            toast.error(errString(err, t("Could not check speaker models")));
        }
    };

    const handleClick = async () => {
        if (!meetingId || busy) return;
        Analytics.trackButtonClick("detect_speakers", "meeting_details");
        setPhase("checking");
        try {
            const existing = await invoke<SpeakerInfo[]>("get_meeting_speakers", { meetingId });
            if (existing.length > 0) {
                setExistingSpeakerCount(existing.length);
                setShowRerunConfirmation(true);
                setPhase("idle");
                return;
            }
        } catch (err) {
            console.warn("Could not inspect existing meeting speakers:", err);
        }
        setPhase("idle");
        await checkModelsAndRun();
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
            toast.error(errString(err, t("Failed to download speaker models")));
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
                title={t('Detect and label speakers in this meeting')}
            >
                {busy ? (
                    <Loader2 className="xl:mr-2 animate-spin" size={18} />
                ) : (
                    <Users className="xl:mr-2" size={18} />
                )}
                <span className="hidden lg:inline">
                    {phase === "diarizing" ? t('Detecting...') : t('Speakers')}
                </span>
            </Button>

            <Dialog
                open={showRerunConfirmation}
                onOpenChange={(open) => { if (!busy) setShowRerunConfirmation(open); }}
            >
                <DialogContent className="sm:max-w-[460px]">
                    <DialogHeader>
                        <DialogTitle className="flex items-center gap-2">
                            <Users className="h-5 w-5 text-[var(--gold)]" />
                            {t('Run speaker detection again?')}
                        </DialogTitle>
                        <DialogDescription>
                            {t('This meeting already has detected speakers. Running detection again replaces the current automatic speaker assignments and may produce a different count. User-confirmed speaker names are preserved.')}
                        </DialogDescription>
                    </DialogHeader>
                    <p className="text-sm text-[var(--fg2)]">
                        {t('Currently detected')}: {existingSpeakerCount}
                    </p>
                    <DialogFooter>
                        <Button variant="outline" onClick={() => setShowRerunConfirmation(false)}>
                            {t('Cancel')}
                        </Button>
                        <Button
                            onClick={async () => {
                                setShowRerunConfirmation(false);
                                await checkModelsAndRun();
                            }}
                            className="bg-[var(--gold)] hover:bg-[var(--gold-active)]"
                        >
                            {t('Run again')}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            <Dialog
                open={showDownload}
                onOpenChange={(o) => { if (!downloading) setShowDownload(o); }}
            >
                <DialogContent className="sm:max-w-[420px]">
                    <DialogHeader>
                        <DialogTitle className="flex items-center gap-2">
                            <Users className="h-5 w-5 text-[var(--gold)]" />
                            {t('Detect Speakers')}
                        </DialogTitle>
                        <DialogDescription>
                            {t('Speaker detection needs a one-time download of the diarization models (~35 MB). They are stored locally and reused for future meetings.')}
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            variant="outline"
                            onClick={() => setShowDownload(false)}
                            disabled={downloading}
                        >
                            {t('Cancel')}
                        </Button>
                        <Button
                            onClick={handleDownloadAndDetect}
                            disabled={downloading}
                            className="bg-[var(--gold)] hover:bg-[var(--gold-active)]"
                        >
                            {downloading ? (
                                <>
                                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                                    {t('Downloading...')}
                                </>
                            ) : (
                                <>
                                    <Download className="h-4 w-4 mr-2" />
                                    {t('Download & Detect')}
                                </>
                            )}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    );
}
