import { toast } from 'sonner';

interface UnsavedNotesOptions {
  level: 'error' | 'warning';
  title: string;
  description: string;
  markdown: string;
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
        Promise.resolve().then(() => navigator.clipboard.writeText(markdown)).then(
          () => toast.success('Notes copied to clipboard'),
          () => toast.error('Failed to copy notes'),
        );
      },
    },
    duration: Infinity,
  });
}
