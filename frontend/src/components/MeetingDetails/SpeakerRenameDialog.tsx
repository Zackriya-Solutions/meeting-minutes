import { useState, useEffect } from "react";
import { toast } from "sonner";
import {
    Dialog,
    DialogContent,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "../ui/dialog";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { useT } from "@/lib/i18n";

interface SpeakerRenameDialogProps {
    open: boolean;
    /** The speaker's current display name (seeds the input). */
    currentName: string;
    /** Whether diarization currently recognizes this voice profile as the local user. */
    currentIsSelf: boolean;
    onOpenChange: (open: boolean) => void;
    /** Persist the new name. May throw — errors surface as a toast. */
    onRename: (displayName: string) => Promise<void> | void;
    /** Persist owner identity on the diarized voice profile. */
    onSelfChange: (isSelf: boolean) => Promise<void> | void;
}

/** Minimal single-field dialog for renaming a diarized speaker. */
export function SpeakerRenameDialog({
    open,
    currentName,
    currentIsSelf,
    onOpenChange,
    onRename,
    onSelfChange,
}: SpeakerRenameDialogProps) {
    const t = useT();
    const [name, setName] = useState(currentName);
    const [isSelf, setIsSelf] = useState(currentIsSelf);
    const [saving, setSaving] = useState(false);

    useEffect(() => {
        if (open) {
            setName(currentName);
            setIsSelf(currentIsSelf);
        }
    }, [open, currentName, currentIsSelf]);

    const handleSave = async () => {
        const trimmed = name.trim();
        if (!trimmed || saving) return;
        setSaving(true);
        try {
            if (trimmed !== currentName.trim()) {
                await onRename(trimmed);
            }
            if (isSelf !== currentIsSelf) {
                await onSelfChange(isSelf);
            }
            onOpenChange(false);
        } catch (err) {
            toast.error(
                typeof err === "string" ? err : (err as any)?.message || t("Failed to rename speaker")
            );
        } finally {
            setSaving(false);
        }
    };

    return (
        <Dialog open={open} onOpenChange={(o) => { if (!saving) onOpenChange(o); }}>
            <DialogContent className="sm:max-w-[360px]">
                <DialogHeader>
                    <DialogTitle>{t('Rename speaker')}</DialogTitle>
                </DialogHeader>
                <div className="py-1">
                    <Input
                        autoFocus
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        onKeyDown={(e) => {
                            if (e.key === "Enter") {
                                e.preventDefault();
                                handleSave();
                            }
                        }}
                        placeholder={t('Speaker name')}
                    />
                </div>
                <label className="flex items-center justify-between gap-4 rounded-lg border border-border px-3 py-3">
                    <span className="min-w-0">
                        <span className="block text-sm font-medium text-foreground">{t('This is me')}</span>
                        <span className="mt-0.5 block text-xs text-muted-foreground">
                            {t('Use this diarized voice to mark your messages')}
                        </span>
                    </span>
                    <Switch
                        checked={isSelf}
                        onCheckedChange={setIsSelf}
                        disabled={saving}
                        aria-label={t('This is me')}
                    />
                </label>
                <DialogFooter>
                    <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
                        {t('Cancel')}
                    </Button>
                    <Button
                        onClick={handleSave}
                        disabled={saving || !name.trim()}
                        className="bg-primary hover:bg-primary/90"
                    >
                        {saving ? t('Saving...') : t('Save')}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
}
