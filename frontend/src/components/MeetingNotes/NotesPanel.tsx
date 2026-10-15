'use client';

import { Component, useCallback, useEffect, useRef, useState, type MouseEvent, type ReactNode } from 'react';
import dynamic from 'next/dynamic';
import type { Block } from '@blocknote/core';
import { Copy, NotebookPen } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { blocksToMarkdownSafely } from '@/lib/blocknote-markdown';
import { createNotesChangeEmitter, editorFailureAction, isEmptyTextBlock, type NotesContent } from '@/lib/meeting-notes';
import type { NotesSaveState } from '@/lib/notes-autosave';
import type { NotesLoadState } from '@/hooks/meeting-details/useMeetingNotes';
import type { BlockNoteEditorInstance } from '@/components/BlockNoteEditor/Editor';

const Editor = dynamic(() => import('@/components/BlockNoteEditor/Editor'), { ssr: false });

const SAVE_STATE_LABEL: Record<NotesSaveState, string> = {
  idle: '',
  saving: 'Saving…',
  saved: 'Saved',
  error: "Couldn't save",
};

interface NotesEditorProps {
  initialBlocks: Block[] | null;
  initialMarkdown: string;
  onChange: (content: NotesContent) => void;
  editable: boolean;
  onFailed: () => void;
}

interface NotesPanelProps extends Omit<NotesEditorProps, 'editable' | 'onFailed'> {
  loadState?: NotesLoadState;
  saveState: NotesSaveState;
  onRetryLoad?: () => void;
  onRetrySave?: () => void;
  onCopy?: () => void;
  editable?: boolean;
  className?: string;
}

/** Catches an editor that throws, so the pane can fall back to markdown or show the load error. */
class EditorBoundary extends Component<{ onError: () => void; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch(error: unknown) {
    console.error('Notes editor failed to render', error);
    this.props.onError();
  }

  render() {
    return this.state.failed ? null : this.props.children;
  }
}

function NotesEditor({ initialBlocks, initialMarkdown, onChange, editable, onFailed }: NotesEditorProps) {
  const startBlocks = initialBlocks && initialBlocks.length > 0 ? initialBlocks : null;
  const [fromMarkdown, setFromMarkdown] = useState(startBlocks === null);
  const [markdownLoaded, setMarkdownLoaded] = useState(false);
  const editorRef = useRef<BlockNoteEditorInstance | null>(null);
  const editorReadyRef = useRef(false);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const [emitter] = useState(() =>
    createNotesChangeEmitter(
      async (blocks) => {
        const editor = editorRef.current;
        if (!editor) return undefined;
        return (await blocksToMarkdownSafely(editor, blocks, { source: 'meeting-notes' })).markdown;
      },
      (content) => onChangeRef.current(content),
    ),
  );

  const handleEditorReady = useCallback(
    (editor: BlockNoteEditorInstance) => {
      editorRef.current = editor;
      // Only the first ready event loads the document; a later one must not re-snapshot typed text.
      if (editorReadyRef.current) return;
      editorReadyRef.current = true;
      if (!fromMarkdown || !initialMarkdown.trim()) {
        emitter.markLoaded(editor.document);
        return;
      }
      // Loading stored markdown is not an edit: it must not trigger a save.
      emitter
        .suppress(async () => {
          const blocks = await editor.tryParseMarkdownToBlocks(initialMarkdown);
          editor.replaceBlocks(editor.document, blocks);
          await new Promise((resolve) => setTimeout(resolve, 0));
          emitter.markLoaded(editor.document);
          setMarkdownLoaded(true);
        })
        .catch((error) => {
          console.error('Failed to load notes from markdown', error);
          onFailed();
        });
    },
    [emitter, fromMarkdown, initialMarkdown, onFailed],
  );

  const handleBlocksChange = useCallback(
    (blocks: Block[]) => {
      void emitter.handle(blocks);
    },
    [emitter],
  );

  const handleEditorError = useCallback(() => {
    if (editorFailureAction(editorReadyRef.current, fromMarkdown) === 'open-from-markdown') {
      setFromMarkdown(true);
    } else {
      onFailed();
    }
  }, [fromMarkdown, onFailed]);

  // Keys typed before the stored markdown replaces the content would be discarded.
  const awaitingMarkdown = fromMarkdown && initialMarkdown.trim() !== '' && !markdownLoaded;
  const isEditable = editable && !awaitingMarkdown;

  // A click on the empty space below the content puts the cursor at the end of the note.
  const handleEmptySpaceMouseDown = (event: MouseEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    const editor = editorRef.current;
    if (!editor || !isEditable) return;
    event.preventDefault();
    try {
      const last = editor.document[editor.document.length - 1];
      if (isEmptyTextBlock(last)) {
        editor.setTextCursorPosition(last, 'end');
      } else {
        const [added] = editor.insertBlocks([{ type: 'paragraph' }], last, 'after');
        editor.setTextCursorPosition(added, 'end');
      }
      editor.focus();
    } catch (error) {
      console.error('Failed to focus the end of the notes', error);
    }
  };

  return (
    <div className="min-h-full" onMouseDown={handleEmptySpaceMouseDown}>
      <EditorBoundary key={fromMarkdown ? 'markdown' : 'blocks'} onError={handleEditorError}>
        <Editor
          initialContent={fromMarkdown ? undefined : startBlocks ?? undefined}
          onChange={handleBlocksChange}
          onEditorReady={handleEditorReady}
          editable={isEditable}
        />
      </EditorBoundary>
    </div>
  );
}

export function NotesPanel({
  loadState = 'ready',
  initialBlocks,
  initialMarkdown,
  saveState,
  onChange,
  onRetryLoad,
  onRetrySave,
  onCopy,
  editable = true,
  className,
}: NotesPanelProps) {
  const [editorFailed, setEditorFailed] = useState(false);
  const handleEditorFailed = useCallback(() => setEditorFailed(true), []);
  useEffect(() => {
    if (loadState !== 'ready') setEditorFailed(false);
  }, [loadState]);
  const showError = loadState === 'error' || (loadState === 'ready' && editorFailed);

  return (
    <div className={cn('flex min-h-0 min-w-0 flex-1 flex-col bg-white', className)}>
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-gray-200 px-4 py-2">
        <div className="flex items-center gap-2 text-sm font-medium text-gray-700">
          <NotebookPen className="h-4 w-4" />
          Notes
          <span className="text-xs font-normal text-gray-500" aria-live={saveState === 'error' ? 'polite' : 'off'}>
            {SAVE_STATE_LABEL[saveState]}
          </span>
          {saveState === 'error' && onRetrySave && (
            <button type="button" onClick={onRetrySave} className="text-xs font-normal text-blue-600 hover:underline">
              Retry
            </button>
          )}
        </div>
        {onCopy && (
          <Button variant="outline" size="sm" onClick={onCopy} title="Copy Notes">
            <Copy />
            <span className="hidden md:inline">Copy</span>
          </Button>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto py-3">
        {loadState === 'loading' && <p className="px-4 text-sm text-gray-500">Loading notes…</p>}
        {showError && (
          <div className="px-4 text-sm text-gray-700">
            <p>Couldn&apos;t load notes.</p>
            {onRetryLoad && (
              <button type="button" onClick={onRetryLoad} className="mt-1 text-blue-600 hover:underline">
                Try again
              </button>
            )}
          </div>
        )}
        {loadState === 'ready' && !editorFailed && (
          <NotesEditor
            initialBlocks={initialBlocks}
            initialMarkdown={initialMarkdown}
            onChange={onChange}
            editable={editable}
            onFailed={handleEditorFailed}
          />
        )}
      </div>
    </div>
  );
}
