import { toast } from 'sonner';

interface UnsavedNotesOptions {
  level: 'error' | 'warning';
  title: string;
  description: string;
  markdown: string;
}

/** Copies notes markdown to the clipboard and tells the user whether it worked. */
export async function copyNotesToClipboard(markdown: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(markdown);
    toast.success('Notes copied to clipboard');
  } catch (error) {
    console.error('Failed to copy notes:', error);
    toast.error('Failed to copy notes');
  }
}

/**
 * Tells the user their notes were not saved and offers to copy them.
 * The toast stays open after copying, so the text can be copied again if needed.
 */
export function notifyUnsavedNotes({ level, title, description, markdown }: UnsavedNotesOptions): void {
  const show = level === 'error' ? toast.error : toast.warning;
  show(title, {
    description,
    action: {
      label: 'Copy notes',
      onClick: (event) => {
        event.preventDefault();
        void copyNotesToClipboard(markdown);
      },
    },
    duration: Infinity,
  });
}
