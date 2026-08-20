"use client"

import { useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Switch } from "./ui/switch"
import { Mic, AlertCircle } from "lucide-react"

interface DictationSettingsValue {
  enabled: boolean;
  hotkey: string | null;
}

const DEFAULT_HOTKEY = "Alt+Shift+D";

export function DictationSettings() {
  const [settings, setSettings] = useState<DictationSettingsValue | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const loaded = await invoke<DictationSettingsValue>("get_dictation_settings");
        if (!cancelled) {
          setSettings(loaded);
        }
      } catch (err) {
        console.error("[DictationSettings] Failed to load settings:", err);
        if (!cancelled) {
          setError("Failed to load live dictation settings.");
          setSettings({ enabled: false, hotkey: DEFAULT_HOTKEY });
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const persist = async (next: DictationSettingsValue) => {
    setIsSaving(true);
    setError(null);
    const previous = settings;
    setSettings(next);
    try {
      await invoke("set_dictation_settings", { settings: next });
    } catch (err) {
      console.error("[DictationSettings] Failed to save settings:", err);
      setError(
        typeof err === "string"
          ? err
          : "Failed to update live dictation. This feature currently requires Linux with AT-SPI accessibility support."
      );
      setSettings(previous);
    } finally {
      setIsSaving(false);
    }
  };

  if (isLoading || !settings) {
    return <div className="max-w-2xl mx-auto p-6">Loading dictation settings...</div>;
  }

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex-1">
          <div className="flex items-center gap-2 mb-2">
            <Mic className="h-5 w-5 text-gray-600" />
            <h3 className="text-lg font-semibold text-gray-900">Live Dictation</h3>
            <span className="px-2 py-0.5 text-xs font-medium bg-yellow-100 text-yellow-800 rounded-full">
              BETA · LINUX
            </span>
          </div>
          <p className="text-sm text-gray-600">
            Transcribe your speech directly into whatever text field currently has focus in any
            application, via the AT-SPI accessibility bus. Password fields are always skipped. If
            a field can&apos;t be typed into, the segment is copied to your clipboard instead.
          </p>
        </div>

        <div className="ml-6">
          <Switch
            checked={settings.enabled}
            disabled={isSaving}
            onCheckedChange={(checked) => persist({ ...settings, enabled: checked })}
          />
        </div>
      </div>

      <div className="flex items-center justify-between border-t border-gray-100 pt-4">
        <div>
          <p className="text-sm font-medium text-gray-900">Global toggle hotkey</p>
          <p className="text-xs text-gray-500">
            Press this key combination anywhere, or use the tray icon menu, to start or stop
            dictation.
          </p>
        </div>
        <kbd className="px-2 py-1 text-xs font-mono bg-gray-100 border border-gray-300 rounded">
          {settings.hotkey ?? DEFAULT_HOTKEY}
        </kbd>
      </div>

      {error && (
        <div className="flex items-start gap-3 p-3 bg-red-50 border border-red-200 rounded-lg">
          <AlertCircle className="h-4 w-4 text-red-600 flex-shrink-0 mt-0.5" />
          <p className="text-sm text-red-800">{error}</p>
        </div>
      )}
    </div>
  );
}
