'use client';

import { useRef, useEffect } from 'react';

interface EditableTitleProps {
  title: string;
  isEditing: boolean;
  onStartEditing: () => void;
  onFinishEditing: () => void;
  onChange: (value: string) => void;
  onDelete?: () => void;
}

export const EditableTitle: React.FC<EditableTitleProps> = ({
  title,
  isEditing,
  onStartEditing,
  onFinishEditing,
  onChange,
  onDelete,
}) => {
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

  return isEditing ? (
    <div className="flex-1">
      <textarea
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
        className="text-2xl font-bold bg-[var(--bg-sheet)] border border-[var(--border-subtle)] focus:outline-none focus:ring-2 ring-[var(--gold-ring)] rounded px-3 py-1 w-full resize-none overflow-hidden"
        style={{ minWidth: '300px', minHeight: '40px' }}
        autoFocus
        rows={1}
      />
    </div>
  ) : (
    <div className="group flex items-center space-x-2 flex-1">
      <h1
        className="text-2xl font-bold cursor-pointer hover:bg-[var(--bg-sheet)] rounded px-1 flex-1 whitespace-pre-wrap"
        onClick={onStartEditing}
      >
        {title}
      </h1>
      <div className="flex space-x-1">
        <button 
          onClick={onStartEditing}
          className="opacity-0 group-hover:opacity-100 transition-opacity duration-200 p-1 hover:bg-[var(--bg-elevated)] rounded"
          title="Edit section title"
        >
          <svg 
            xmlns="http://www.w3.org/2000/svg" 
            width="16" 
            height="16" 
            viewBox="0 0 24 24" 
            fill="none" 
            stroke="currentColor" 
            strokeWidth="2" 
            strokeLinecap="round" 
            strokeLinejoin="round"
          >
            <path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z" />
          </svg>
        </button>
        {onDelete && (
          <button 
            onClick={onDelete}
            className="opacity-0 group-hover:opacity-100 transition-opacity duration-200 p-1 hover:bg-[var(--bg-elevated)] rounded text-[var(--danger)]"
            title="Delete section"
          >
            <svg 
              xmlns="http://www.w3.org/2000/svg" 
              width="16" 
              height="16" 
              viewBox="0 0 24 24" 
              fill="none" 
              stroke="currentColor" 
              strokeWidth="2" 
              strokeLinecap="round" 
              strokeLinejoin="round"
            >
              <path d="M3 6h18" />
              <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
              <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
};
