import { useState, useEffect } from "react";
import { toast } from "sonner";
import {
    Dialog,
    DialogContent,
    DialogFooter,
    DialogTitle,
} from "../ui/fluid-dialog";
import { Button } from "../ui/fluid-button";
import { Input } from "../ui/fluid-input";
import { useT } from "@/lib/i18n";
import { ShapeProvider } from "@/lib/shape-context";

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
        if (open) {
            setName(currentName);
        }
    }, [open, currentName]);

    const handleSave = async () => {
        const trimmed = name.trim();
        if (!trimmed || saving) return;
        setSaving(true);
        try {
            if (trimmed !== currentName.trim()) {
                await onRename(trimmed);
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
        <ShapeProvider defaultShape="rounded" publishToRoot={false}>
            <Dialog open={open} onOpenChange={(o) => { if (!saving) onOpenChange(o); }}>
                <DialogContent size="sm">
                    <form onSubmit={(event) => { event.preventDefault(); void handleSave(); }}>
                        <DialogTitle>{t('Rename speaker')}</DialogTitle>
                        <Input
                            autoFocus
                            className="mt-4"
                            value={name}
                            onChange={(e) => setName(e.target.value)}
                            placeholder={t('Speaker name')}
                            disabled={saving}
                        />
                        <DialogFooter>
                            <Button
                                type="button"
                                variant="secondary"
                                onClick={() => onOpenChange(false)}
                                disabled={saving}
                            >
                                {t('Cancel')}
                            </Button>
                            <Button
                                type="submit"
                                variant="primary"
                                loading={saving}
                                disabled={!name.trim()}
                            >
                                {t('Save')}
                            </Button>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
        </ShapeProvider>
    );
}
