"use client";

import { useEffect, useRef } from "react";
import type { PartialBlock, Block } from "@blocknote/core";
import { useCreateBlockNote } from "@blocknote/react";
import { BlockNoteView } from "@blocknote/shadcn";
import "@blocknote/shadcn/style.css";
import "@blocknote/core/fonts/inter.css";

interface EditorProps {
  initialContent?: Block[];
  onChange?: (blocks: Block[]) => void;
  editable?: boolean;
}

export default function Editor({ initialContent, onChange, editable = true }: EditorProps) {
  console.log('📝 EDITOR: Initializing BlockNote editor with blocks:', {
    hasContent: !!initialContent,
    blocksCount: initialContent?.length || 0,
    editable
  });

  const editor = useCreateBlockNote({
    initialContent: initialContent as PartialBlock[] | undefined,
  });

  console.log('📝 EDITOR: BlockNote editor created successfully');

  // Expose blocksToMarkdown method with error handling, wrapping only once.
  const originalBlocksToMarkdownRef = useRef<typeof editor.blocksToMarkdownLossy | null>(null);
  useEffect(() => {
    if (originalBlocksToMarkdownRef.current) return;

    originalBlocksToMarkdownRef.current = editor.blocksToMarkdownLossy.bind(editor);
    (editor as any).blocksToMarkdownLossy = async (blocks: Block[]) => {
      try {
        return await originalBlocksToMarkdownRef.current!(blocks);
      } catch (error) {
        console.error('❌ EDITOR: Failed to convert blocks to markdown:', error);
        return '';
      }
    };
  }, [editor]);

  // Handle content changes
  useEffect(() => {
    if (!onChange) return;

    const handleChange = () => {
      console.log('📝 EDITOR: Content changed, notifying parent...', {
        blocksCount: editor.document.length
      });
      onChange(editor.document);
    };

    const unsubscribe = editor.onChange(handleChange);

    return () => {
      if (typeof unsubscribe === 'function') {
        console.log('📝 EDITOR: Cleaning up onChange listener');
        unsubscribe();
      }
    };
  }, [editor, onChange]);

  return <BlockNoteView editor={editor} editable={editable} theme="light" />;
}
