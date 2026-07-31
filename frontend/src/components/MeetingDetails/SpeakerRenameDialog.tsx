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
import { useT } from "@/lib/i18n";

interface SpeakerRenameDialogProps {
    open: boolean;
    /** The speaker's current display name (seeds the input). */
    currentName: string;
    onOpenChange: (open: boolean) => void;
    /** Persist the new name. May throw — errors surface as a toast. */
    onRename: (displayName: string) => Promise<void> | void;
}

/** Minimal single-field dialog for renaming a diarized speaker. */
export function SpeakerRenameDialog({
    open,
    currentName,
    onOpenChange,
    onRename,
}: SpeakerRenameDialogProps) {
    const t = useT();
    const [name, setName] = useState(currentName);
    const [saving, setSaving] = useState(false);

    useEffect(() => {
        if (open) setName(currentName);
    }, [open, currentName]);

    const handleSave = async () => {
        const trimmed = name.trim();
        if (!trimmed || saving) return;
        setSaving(true);
        try {
            await onRename(trimmed);
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
