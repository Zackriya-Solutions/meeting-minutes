"use client";

import { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Switch } from '@/components/ui/switch';
import {
  SUMMARY_CONTEXT_MAX_CHARS,
  SUMMARY_CONTEXT_TEXTAREA_CLASS,
  inheritedContextKey,
  inheritedContextName,
  summaryContextCounter,
  type InheritedContext,
} from '@/lib/summary-context';

interface SummaryContextBoxProps {
  text: string;
  onTextChange: (value: string) => void;
  useInherited: boolean;
  onUseInheritedChange: (value: boolean) => void;
  inherited: InheritedContext[];
  isLoaded: boolean;
  onEditInherited: (level: InheritedContext) => void;
}

export function SummaryContextBox({
  text,
  onTextChange,
  useInherited,
  onUseInheritedChange,
  inherited,
  isLoaded,
  onEditInherited,
}: SummaryContextBoxProps) {
  const [expandedKey, setExpandedKey] = useState<string | null>(null);
  const expanded = inherited.find(level => inheritedContextKey(level) === expandedKey) ?? null;
  const counter = summaryContextCounter(text.length);

  return (
    <div className="p-1 border-t border-gray-200">
      {inherited.length > 0 && (
        <div className="mb-1 px-1 text-xs text-gray-600">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <span className="text-gray-500">Also sent:</span>
            {inherited.map(level => {
              const key = inheritedContextKey(level);
              const isOpen = key === expandedKey;
              return (
                <button
                  key={key}
                  type="button"
                  aria-expanded={isOpen}
                  onClick={() => setExpandedKey(isOpen ? null : key)}
                  className={`inline-flex items-center gap-0.5 hover:text-gray-900 ${useInherited ? '' : 'text-gray-400'}`}
                >
                  {inheritedContextName(level)}
                  {isOpen ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
                </button>
              );
            })}
            <label className="ml-auto inline-flex items-center gap-2">
              <Switch
                checked={useInherited}
                onCheckedChange={onUseInheritedChange}
                aria-label="Use inherited"
              />
              <span>Use inherited</span>
            </label>
          </div>
          {expanded && (
            <div className="mt-1 rounded-md border border-gray-200 bg-gray-50 p-2">
              <p className="max-h-32 overflow-y-auto whitespace-pre-wrap text-gray-700">{expanded.context}</p>
              <button
                type="button"
                onClick={() => onEditInherited(expanded)}
                className="mt-1 text-blue-600 hover:underline"
              >
                Edit
              </button>
            </div>
          )}
        </div>
      )}
      <textarea
        placeholder="Add context for AI summary. For example people involved, meeting overview, objective etc..."
        className={`${SUMMARY_CONTEXT_TEXTAREA_CLASS} min-h-[80px]`}
        value={text}
        onChange={(e) => onTextChange(e.target.value)}
        disabled={!isLoaded}
        maxLength={SUMMARY_CONTEXT_MAX_CHARS}
      />
      {counter && <div className="px-1 text-right text-xs text-gray-500">{counter}</div>}
    </div>
  );
}
