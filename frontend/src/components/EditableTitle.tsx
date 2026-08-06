'use client';

import { useRef, useEffect } from 'react';
import { Pencil, Trash2 } from 'lucide-react';

interface EditableTitleProps {
  title: string;
  isEditing: boolean;
  onStartEditing: () => void;
  onFinishEditing: () => void;
  onChange: (value: string) => void;
  onDelete?: () => void;
}

/**
 * The meeting title as a document heading. Serif, because on the review
 * surface this is the top of a document rather than a UI label.
 */
export const EditableTitle: React.FC<EditableTitleProps> = ({
  title,
  isEditing,
  onStartEditing,
  onFinishEditing,
  onChange,
  onDelete,
}) => {
  const titleInputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (titleInputRef.current && isEditing) {
      titleInputRef.current.style.height = 'auto';
      titleInputRef.current.style.height = `${titleInputRef.current.scrollHeight}px`;
    }
  }, [title, isEditing]);

  if (isEditing) {
    return (
      <textarea
        ref={titleInputRef}
        value={title}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onFinishEditing}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            onFinishEditing();
          }
          if (e.key === 'Escape') onFinishEditing();
        }}
        rows={1}
        aria-label="Meeting title"
        className="w-full resize-none overflow-hidden rounded-md border border-line-strong bg-canvas px-2 py-1 font-serif text-2xl font-semibold text-ink focus:border-brand"
        autoFocus
      />
    );
  }

  return (
    <div className="group flex min-w-0 items-start gap-1">
      <h1
        onClick={onStartEditing}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            onStartEditing();
          }
        }}
        title="Click to rename"
        className="min-w-0 flex-1 cursor-text text-balance rounded-md px-2 py-1 font-serif text-2xl font-semibold text-ink transition-colors duration-fast hover:bg-ink/[0.04]"
      >
        {title || 'Untitled meeting'}
      </h1>
      <div className="flex shrink-0 items-center gap-0.5 pt-1.5 opacity-0 transition-opacity duration-fast group-hover:opacity-100 group-focus-within:opacity-100">
        <button
          onClick={onStartEditing}
          aria-label="Rename meeting"
          className="flex h-7 w-7 items-center justify-center rounded-md text-ink-faint transition-colors duration-fast hover:bg-ink/5 hover:text-ink"
        >
          <Pencil className="h-3.5 w-3.5" />
        </button>
        {onDelete && (
          <button
            onClick={onDelete}
            aria-label="Delete meeting"
            className="flex h-7 w-7 items-center justify-center rounded-md text-ink-faint transition-colors duration-fast hover:bg-danger-soft hover:text-danger-ink"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        )}
      </div>
    </div>
  );
};
