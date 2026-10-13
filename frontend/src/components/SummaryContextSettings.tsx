'use client';

import { useEffect, useState } from 'react';
import { FileText } from 'lucide-react';
import { toast } from 'sonner';
import { createDebouncedSaver } from '@/lib/debouncedSaver';
import {
  SUMMARY_CONTEXT_MAX_CHARS,
  SUMMARY_CONTEXT_SAVE_DELAY_MS,
  summaryContextCounter,
} from '@/lib/summary-context';
import { summaryContextService } from '@/services/summaryContextService';

const GLOBAL_KEY = 'global';

export function SummaryContextSettings() {
  const [text, setText] = useState('');
  const [isLoaded, setIsLoaded] = useState(false);
  const [saver] = useState(() => createDebouncedSaver<string>(
    (_key, value) => summaryContextService.saveGlobal(value),
    error => {
      console.error('Failed to save summary context:', error);
      toast.error('Failed to save summary context', { description: String(error) });
    },
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
        if (cancelled) return;
        console.error('Failed to load summary context:', error);
        toast.error('Failed to load summary context', { description: String(error) });
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
        className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[120px] resize-y disabled:bg-gray-50"
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          saver.schedule(GLOBAL_KEY, e.target.value);
        }}
        disabled={!isLoaded}
        maxLength={SUMMARY_CONTEXT_MAX_CHARS}
      />
      {counter && <div className="text-right text-xs text-gray-500">{counter}</div>}
    </div>
  );
}
