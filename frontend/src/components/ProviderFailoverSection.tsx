'use client';

import { useState } from 'react';
import { ChevronUp, ChevronDown, Plus, Trash2, ShieldAlert, RotateCcw } from 'lucide-react';
import { useTranslations } from 'next-intl';
import { Switch } from '@/components/ui/switch';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useProviderChain, MAX_CHAIN_LENGTH, ProviderChainEntry } from '@/hooks/useProviderChain';

// PR-32: Provider failover chain editor.
// Default behaviour (toggle off) = single primary provider, no failover.
// Opt-in only; explicit warning shown to prevent accidental double-billing.

const KNOWN_PROVIDERS = ['openai', 'claude', 'groq', 'ollama', 'openrouter'] as const;

export function ProviderFailoverSection() {
  const t = useTranslations('settings');
  const chain = useProviderChain();
  const [newProvider, setNewProvider] = useState<string>('ollama');
  const [newModel, setNewModel] = useState<string>('');

  const handleAdd = () => {
    const model = newModel.trim();
    if (!model) return;
    chain.addEntry({ provider: newProvider, model });
    setNewModel('');
  };

  const handleReset = () => {
    chain.reset();
    setNewProvider('ollama');
    setNewModel('');
  };
  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-center justify-between mb-2">
        <div>
          <h3 className="text-lg font-semibold text-gray-900">{t('summary.failover_title')}</h3>
          <p className="text-sm text-gray-600 mt-1">{t('summary.failover_hint')}</p>
        </div>
        <Switch checked={chain.enabled} onCheckedChange={chain.setEnabled} />
      </div>

      {chain.enabled && (
        <div className="mt-4 rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800 flex gap-2">
          <ShieldAlert size={16} className="mt-0.5 flex-shrink-0" />
          <span>{t('summary.failover_warning')}</span>
        </div>
      )}

      {chain.enabled && (
        <div className="mt-4 space-y-2">
          {chain.entries.length === 0 ? (
            <p className="text-sm text-gray-500 italic">{t('summary.failover_empty')}</p>
          ) : (
            <ol className="space-y-2">
              {chain.entries.map((entry, index) => (
                <li
                  key={`${entry.provider}-${entry.model}-${index}`}
                  className="flex items-center gap-2 rounded-md border border-gray-200 bg-gray-50 px-3 py-2"
                >
                  <span className="text-xs font-mono text-gray-500 w-6">{index + 1}.</span>
                  <span className="font-medium text-gray-900">{entry.provider}</span>
                  <span className="text-gray-500">/</span>
                  <span className="text-gray-700">{entry.model}</span>
                  <div className="ml-auto flex items-center gap-1">
                    <button
                      type="button"
                      aria-label={t('summary.failover_move_up')}
                      disabled={index === 0}
                      onClick={() => chain.moveEntry(index, index - 1)}
                      className="rounded p-1 text-gray-500 hover:bg-gray-200 disabled:opacity-30 disabled:cursor-not-allowed"
                    >
                      <ChevronUp size={14} />
                    </button>
                    <button
                      type="button"
                      aria-label={t('summary.failover_move_down')}
                      disabled={index === chain.entries.length - 1}
                      onClick={() => chain.moveEntry(index, index + 1)}
                      className="rounded p-1 text-gray-500 hover:bg-gray-200 disabled:opacity-30 disabled:cursor-not-allowed"
                    >
                      <ChevronDown size={14} />
                    </button>
                    <button
                      type="button"
                      aria-label={t('summary.failover_remove')}
                      onClick={() => chain.removeEntry(index)}
                      className="rounded p-1 text-red-500 hover:bg-red-50"
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                </li>
              ))}
            </ol>
          )}

          {chain.entries.length < MAX_CHAIN_LENGTH && (
            <div className="flex items-center gap-2 mt-3 pt-3 border-t border-gray-100">
              <Select value={newProvider} onValueChange={setNewProvider}>
                <SelectTrigger className="w-[160px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {KNOWN_PROVIDERS.map(p => (
                    <SelectItem key={p} value={p}>{p}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Input
                placeholder={t('summary.failover_model_placeholder')}
                value={newModel}
                onChange={(e) => setNewModel(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') handleAdd(); }}
                className="flex-1"
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleAdd}
                disabled={!newModel.trim()}
              >
                <Plus size={14} className="mr-1" />
                {t('summary.failover_add')}
              </Button>
            </div>
          )}

          <div className="mt-3 flex justify-end">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={handleReset}
            >
              <RotateCcw size={14} className="mr-1" />
              {t('summary.failover_reset')}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
