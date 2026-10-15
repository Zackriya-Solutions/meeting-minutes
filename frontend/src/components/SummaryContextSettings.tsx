'use client';

import { useEffect, useState } from 'react';
import { FileText } from 'lucide-react';
import { createDebouncedSaver } from '@/lib/debouncedSaver';
import {
  SUMMARY_CONTEXT_MAX_CHARS,
  SUMMARY_CONTEXT_SAVE_DELAY_MS,
  SUMMARY_CONTEXT_TEXTAREA_CLASS,
  reportSummaryContextError,
  summaryContextCounter,
} from '@/lib/summary-context';
import { summaryContextService } from '@/services/summaryContextService';

export function SummaryContextSettings() {
  const [text, setText] = useState('');
  const [isLoaded, setIsLoaded] = useState(false);
  const [saver] = useState(() => createDebouncedSaver<string>(
    (_key, value) => summaryContextService.saveGlobal(value),
    error => reportSummaryContextError('save', error),
    SUMMARY_CONTEXT_SAVE_DELAY_MS,
  ));

  useEffect(() => {
    let cancelled = false;
    summaryContextService.getGlobal()
      .then(saved => {
        if (cancelled) return;
        setText(saved);
        setIsLoaded(true);
      })
      .catch(error => {
        if (!cancelled) reportSummaryContextError('load', error);
      });
    return () => {
      cancelled = true;
      void saver.flush();
    };
  }, [saver]);

  const counter = summaryContextCounter(text.length);

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-center gap-2 mb-2">
        <FileText size={18} className="text-gray-500" />
        <h3 className="text-lg font-semibold text-gray-900">Summary Context</h3>
      </div>
      <p className="text-sm text-gray-600 mb-4">
        Sent with every summary. Use it for things that are always true: who you are, your organization, names, and terms.
      </p>
      <textarea
        aria-label="Summary context for every meeting"
        className={`${SUMMARY_CONTEXT_TEXTAREA_CLASS} min-h-[120px]`}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          saver.schedule('global', e.target.value);
        }}
        disabled={!isLoaded}
        maxLength={SUMMARY_CONTEXT_MAX_CHARS}
      />
      {counter && <div className="text-right text-xs text-gray-500">{counter}</div>}
    </div>
  );
}
