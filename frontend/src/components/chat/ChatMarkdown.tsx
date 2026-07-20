"use client";

import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import 'katex/dist/katex.min.css';
import { cn } from '@/lib/utils';

/**
 * Renders assistant/chat text as Markdown (GFM: lists, tables, code, bold/italic,
 * links) with LaTeX math via KaTeX ($…$ inline, $$…$$ block). Used for meeting-chat
 * answers, which previously rendered as raw pre-wrapped text. Styling lives in the
 * `.mm-md` block in globals.css so it stays consistent in the dark theme.
 */
export function ChatMarkdown({ content, className }: { content: string; className?: string }) {
  return (
    <div className={cn('mm-md', className)}>
      <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeKatex]}>
        {content}
      </ReactMarkdown>
    </div>
  );
}
