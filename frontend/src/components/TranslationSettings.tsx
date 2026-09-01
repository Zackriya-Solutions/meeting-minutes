'use client';

import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { TranslationSettings as TranslationSettingsValue } from '@/lib/translation';

export function TranslationSettings() {
  const { translationSettings, setTranslationSettings } = useConfig();
  const [draft, setDraft] = useState<TranslationSettingsValue>(translationSettings);

  useEffect(() => setDraft(translationSettings), [translationSettings]);

  const save = async () => {
    try {
      await setTranslationSettings(draft);
      toast.success('Translation settings saved');
    } catch (error) {
      toast.error('Could not save translation settings', { description: String(error) });
    }
  };

  return (
    <section className="mt-6 rounded-lg border border-gray-200 bg-white p-6 shadow-sm">
      <h2 className="text-lg font-semibold text-gray-900">Live translation</h2>
      <p className="mt-1 text-sm text-gray-500">Configure the provider used for new finalized transcript segments.</p>
      <div className="mt-4 grid gap-4 md:grid-cols-2">
        <label className="text-sm text-gray-700">
          Engine
          <select
            value={draft.engine}
            onChange={event => setDraft({ ...draft, engine: event.target.value as TranslationSettingsValue['engine'] })}
            className="mt-1 block h-10 w-full rounded-md border border-gray-300 bg-white px-3"
          >
            <option value="ollama">Ollama</option>
            <option value="libretranslate">LibreTranslate</option>
          </select>
        </label>
        <label className="text-sm text-gray-700">
          Default target language
          <input
            value={draft.targetLanguage}
            onChange={event => setDraft({ ...draft, targetLanguage: event.target.value })}
            className="mt-1 block h-10 w-full rounded-md border border-gray-300 px-3"
            placeholder="pt-BR"
          />
        </label>
        <label className="text-sm text-gray-700">
          LibreTranslate endpoint
          <input
            value={draft.libretranslateEndpoint}
            onChange={event => setDraft({ ...draft, libretranslateEndpoint: event.target.value })}
            className="mt-1 block h-10 w-full rounded-md border border-gray-300 px-3"
            placeholder="http://localhost:5000"
          />
        </label>
        <label className="text-sm text-gray-700">
          LibreTranslate API key
          <input
            type="password"
            value={draft.libretranslateApiKey}
            onChange={event => setDraft({ ...draft, libretranslateApiKey: event.target.value })}
            className="mt-1 block h-10 w-full rounded-md border border-gray-300 px-3"
            placeholder="Optional"
            autoComplete="off"
          />
        </label>
        <label className="text-sm text-gray-700">
          Ollama endpoint
          <input
            value={draft.ollamaEndpoint}
            onChange={event => setDraft({ ...draft, ollamaEndpoint: event.target.value })}
            className="mt-1 block h-10 w-full rounded-md border border-gray-300 px-3"
            placeholder="http://localhost:11434"
          />
        </label>
        <label className="text-sm text-gray-700 md:col-span-2">
          Ollama translation model
          <input
            value={draft.ollamaModel}
            onChange={event => setDraft({ ...draft, ollamaModel: event.target.value })}
            className="mt-1 block h-10 w-full rounded-md border border-gray-300 px-3"
            placeholder="aya-expanse:latest"
          />
        </label>
      </div>
      <button type="button" onClick={save} className="mt-5 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700">
        Save translation settings
      </button>
    </section>
  );
}
