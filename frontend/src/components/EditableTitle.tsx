'use client';

import { useRef, useEffect } from 'react';
import { useT } from '@/lib/i18n';
import { Pencil, Trash2 } from '@/components/deslop-icons';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';

interface EditableTitleProps {
  title: string;
  isEditing: boolean;
  onStartEditing: () => void;
  onFinishEditing: () => void;
  onChange: (value: string) => void;
  onDelete?: () => void;
  showEditButton?: boolean;
  seamlessEditing?: boolean;
}

export const EditableTitle: React.FC<EditableTitleProps> = ({
  title,
  isEditing,
  onStartEditing,
  onFinishEditing,
  onChange,
  onDelete,
  showEditButton = true,
  seamlessEditing = false,
}) => {
  const t = useT();
  const titleInputRef = useRef<HTMLTextAreaElement>(null);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      onFinishEditing();
    }
  };

  // Auto-resize textarea height based on content
  useEffect(() => {
    if (titleInputRef.current && isEditing) {
      titleInputRef.current.style.height = 'auto';
      titleInputRef.current.style.height = `${titleInputRef.current.scrollHeight}px`;
    }
  }, [title, isEditing]);

  // Enter rename mode at the natural continuation point instead of placing the
  // caret before the first character. Keep this separate from the resize effect
  // so typing does not repeatedly move the caret back to the end.
  useEffect(() => {
    if (!isEditing || !titleInputRef.current) return;

    const input = titleInputRef.current;
    input.focus({ preventScroll: true });
    const end = input.value.length;
    input.setSelectionRange(end, end);
  }, [isEditing]);

  return isEditing ? (
    <div className="flex-1">
      <Textarea
        ref={titleInputRef}
        value={title}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onFinishEditing}
        onKeyDown={(e) => {
          // Allow Enter for new line only with Shift key
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            onFinishEditing();
          }
        }}
        className={`memento-screen-title w-full resize-none overflow-hidden ${
          seamlessEditing
            ? 'min-h-0 rounded-none border-0 bg-transparent p-0 shadow-none outline-none ring-0 focus-visible:outline-none focus-visible:ring-0'
            : ''
        }`}
        style={seamlessEditing ? undefined : { minWidth: '300px', minHeight: '40px' }}
        autoFocus
        rows={1}
      />
    </div>
  ) : (
    <div className="group flex flex-1 items-center space-x-2 text-left">
      <h1
        className={`memento-screen-title flex-1 cursor-pointer whitespace-pre-wrap rounded text-left ${
          seamlessEditing ? '' : 'hover:bg-background'
        }`}
        onClick={onStartEditing}
      >
        {title}
      </h1>
      <div className="flex space-x-1">
        {showEditButton && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={onStartEditing}
            className="opacity-0 transition-opacity duration-200 group-hover:opacity-100"
            title={t('Edit section title')}
          >
            <Pencil />
          </Button>
        )}
        {onDelete && (
          <Button
            type="button"
            variant="ghost"
            size="icon"
            onClick={onDelete}
            className="text-destructive opacity-0 transition-opacity duration-200 group-hover:opacity-100"
            title={t('Delete section')}
          >
            <Trash2 />
          </Button>
        )}
      </div>
    </div>
  );
};
