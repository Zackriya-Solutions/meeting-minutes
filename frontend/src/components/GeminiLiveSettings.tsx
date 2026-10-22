// Settings panel for the Gemini Live cloud transcription engine. Fork addition.
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Eye, EyeOff, Loader2, Wifi } from 'lucide-react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Textarea } from './ui/textarea';
import { Button } from './ui/button';
import { Label } from './ui/label';
import {
  activateGeminiLive,
  GeminiLiveSettings as Settings,
  GeminiLiveSettingsView,
  getGeminiLiveSettings,
  MAX_VOCABULARY_TERMS,
  saveGeminiLiveSettings,
  testGeminiLiveConnection,
} from '@/lib/gemini-live';

interface GeminiLiveSettingsProps {
  isActive: boolean;
  onActivated: () => void;
}

const parseVocabulary = (text: string) =>
  text.split(/[\n,]/).map((t) => t.trim()).filter(Boolean);

export function GeminiLiveSettings({ isActive, onActivated }: GeminiLiveSettingsProps) {
  const [view, setView] = useState<GeminiLiveSettingsView | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [vocabularyText, setVocabularyText] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [showApiKey, setShowApiKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);

  const load = async () => {
    try {
      const loaded = await getGeminiLiveSettings();
      setView(loaded);
      setSettings({
        languageMode: loaded.languageMode,
        customVocabulary: loaded.customVocabulary,
        transcriptionMode: loaded.transcriptionMode,
        fallbackProvider: loaded.fallbackProvider,
      });
      setVocabularyText(loaded.customVocabulary.join('\n'));
    } catch (error) {
      console.error('Failed to load Gemini Live settings:', error);
      toast.error('Could not load Gemini Live settings', { description: String(error) });
    }
  };

  useEffect(() => {
    load();
  }, []);

  if (!settings || !view) {
    return <div className="text-sm text-gray-500 mt-4">Loading Gemini Live settings…</div>;
  }

  const vocabulary = parseVocabulary(vocabularyText);
  const keyAvailable = apiKey.trim() !== '' || view.hasApiKey || view.hasSummaryApiKey;

  const handleTest = async () => {
    setTesting(true);
    try {
      const message = await testGeminiLiveConnection(apiKey.trim() || null);
      toast.success('Gemini Live works', { description: message });
    } catch (error) {
      toast.error('Gemini Live connection failed', { description: String(error) });
    } finally {
      setTesting(false);
    }
  };

  const handleSave = async () => {
    if (!keyAvailable) {
      toast.error('Add your Gemini API key first');
      return;
    }
    setSaving(true);
    try {
      await saveGeminiLiveSettings({ ...settings, customVocabulary: vocabulary }, apiKey.trim() || null);
      await activateGeminiLive();
      setApiKey('');
      await load();
      onActivated();
      toast.success('Gemini Live is now your transcription engine');
    } catch (error) {
      toast.error('Failed to save Gemini Live settings', { description: String(error) });
    } finally {
      setSaving(false);
    }
  };

  const handleRemoveKey = async () => {
    try {
      await saveGeminiLiveSettings(settings, '');
      await load();
      toast.success('Saved Gemini API key removed');
    } catch (error) {
      toast.error('Failed to remove key', { description: String(error) });
    }
  };

  return (
    <div className="mt-6 space-y-5">
      <div className="rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900">
        Cloud engine: meeting audio is sent to Google for transcription. If Gemini is
        unreachable, Meetily switches to the local backup engine automatically.
      </div>

      <div>
        <Label className="block text-sm font-medium text-gray-700 mb-1">Gemini API key</Label>
        <div className="relative mx-1">
          <Input
            type={showApiKey ? 'text' : 'password'}
            className="pr-10"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={view.hasApiKey ? `Saved key ending in …${view.apiKeyHint ?? ''}` : 'Paste your Gemini API key'}
          />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="absolute inset-y-0 right-0"
            onClick={() => setShowApiKey(!showApiKey)}
          >
            {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
          </Button>
        </div>
        <p className="mt-1 text-xs text-gray-500 mx-1">
          {view.hasApiKey
            ? 'Leave empty to keep the saved key.'
            : view.hasSummaryApiKey
              ? 'No key here yet: the Gemini key from your summary settings will be used.'
              : 'Get a key at aistudio.google.com.'}
          {view.hasApiKey && (
            <button type="button" className="ml-2 text-red-600 hover:underline" onClick={handleRemoveKey}>
              Remove saved key
            </button>
          )}
        </p>
        <div className="mx-1 mt-2">
          <Button type="button" variant="outline" size="sm" onClick={handleTest} disabled={testing || !keyAvailable}>
            {testing ? <Loader2 className="h-4 w-4 mr-2 animate-spin" /> : <Wifi className="h-4 w-4 mr-2" />}
            Test connection
          </Button>
        </div>
      </div>

      <div>
        <Label className="block text-sm font-medium text-gray-700 mb-1">Hindi output script</Label>
        <div className="mx-1">
          <Select
            value={settings.languageMode}
            onValueChange={(v) => setSettings({ ...settings, languageMode: v as Settings['languageMode'] })}
          >
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="roman">Roman / Hinglish (“chaliye start karte hain”)</SelectItem>
              <SelectItem value="devanagari">Devanagari (“चलिए स्टार्ट करते हैं”)</SelectItem>
              <SelectItem value="auto">Automatic (may mix both scripts)</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      <div>
        <Label className="block text-sm font-medium text-gray-700 mb-1">Transcript style</Label>
        <div className="mx-1">
          <Select
            value={settings.transcriptionMode}
            onValueChange={(v) => setSettings({ ...settings, transcriptionMode: v as Settings['transcriptionMode'] })}
          >
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="VERBATIM">Verbatim (every word, including fillers)</SelectItem>
              <SelectItem value="SMART">Smart (removes fillers, cleans formatting)</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      <div>
        <Label className="block text-sm font-medium text-gray-700 mb-1">Custom vocabulary</Label>
        <div className="mx-1">
          <Textarea
            rows={5}
            value={vocabularyText}
            onChange={(e) => setVocabularyText(e.target.value)}
            placeholder={'One term per line, e.g.\nMeetily\nKubernetes\nnames of colleagues and clients'}
          />
          <p className={`mt-1 text-xs ${vocabulary.length > MAX_VOCABULARY_TERMS ? 'text-red-600' : 'text-gray-500'}`}>
            {vocabulary.length} / {MAX_VOCABULARY_TERMS} terms. Names, products and jargon work best; keep
            the list short (under ~100) for best accuracy.
          </p>
        </div>
      </div>

      <div>
        <Label className="block text-sm font-medium text-gray-700 mb-1">Backup engine (offline / errors)</Label>
        <div className="mx-1">
          <Select
            value={settings.fallbackProvider}
            onValueChange={(v) => setSettings({ ...settings, fallbackProvider: v as Settings['fallbackProvider'] })}
          >
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="parakeet">Parakeet (local)</SelectItem>
              <SelectItem value="localWhisper">Whisper (local)</SelectItem>
            </SelectContent>
          </Select>
          <p className="mt-1 text-xs text-gray-500">
            A model for this engine must be downloaded (Parakeet usually is, from setup). To download
            one, pick the engine in the list above, download a model, then come back to Gemini Live
            and click Save again.
          </p>
        </div>
      </div>

      <div className="mx-1 flex items-center gap-3">
        <Button type="button" onClick={handleSave} disabled={saving || vocabulary.length > MAX_VOCABULARY_TERMS}>
          {saving && <Loader2 className="h-4 w-4 mr-2 animate-spin" />}
          {isActive ? 'Save settings' : 'Save and use Gemini Live'}
        </Button>
        {isActive && <span className="text-sm text-green-700">✓ Gemini Live is active</span>}
      </div>
    </div>
  );
}
