'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Switch } from '@/components/ui/switch';
import { toast } from 'sonner';

interface Preferences {
  enabled: boolean;
  browser_calls: boolean;
  stop_when_call_ends: boolean;
}
interface Settings {
  preferences: Preferences;
  supported: boolean;
}

export function CallDetectionSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setError(null);
    try {
      setSettings(await invoke<Settings>('get_call_detection_settings'));
    } catch {
      setError('Could not load call detection settings. Try again.');
    }
  };
  useEffect(() => { void load(); }, []);

  const save = async (change: Partial<Preferences>) => {
    if (!settings || saving) return;
    setSaving(true);
    try {
      const preferences = { ...settings.preferences, ...change };
      await invoke('set_call_detection_preferences', { preferences });
      setSettings({ ...settings, preferences });
      setError(null);
    } catch {
      toast.error('Could not save call detection settings. Try again.');
    } finally {
      setSaving(false);
    }
  };

  return (
    <section aria-labelledby="call-detection-heading" className="space-y-4 border-t pt-6">
      <h4 id="call-detection-heading" className="text-base font-medium text-gray-900">Automatic call recording</h4>
      {error ? (
        <div role="alert" className="text-sm text-red-700">
          {error} <button type="button" onClick={load} className="underline underline-offset-2">Try again</button>
        </div>
      ) : !settings ? (
        <p role="status" className="text-sm text-gray-600">Loading call detection settings…</p>
      ) : !settings.supported ? (
        <p className="text-sm text-gray-600">Call detection is available on macOS 14.2 or later.</p>
      ) : (
        <>
          <div className="flex items-start justify-between gap-4">
            <div>
              <label htmlFor="auto-record-calls" className="font-medium">Start recording when a call is detected</label>
              <p id="auto-record-description" className="text-sm text-gray-600 max-w-prose">
                Detects microphone use in Zoom, Teams, Slack, FaceTime and Webex while Meetily is running.
                Recordings use your selected devices, transcription model and save settings. Let participants know you are recording.
              </p>
            </div>
            <Switch id="auto-record-calls" aria-describedby="auto-record-description" checked={settings.preferences.enabled} onCheckedChange={enabled => save({ enabled })} disabled={saving} />
          </div>
          <div className="flex items-start justify-between gap-4">
            <div>
              <label htmlFor="detect-browser-calls" className="font-medium">Include browser calls</label>
              <p id="browser-call-description" className="text-sm text-gray-600 max-w-prose">
                Recognises Meet, Zoom and Teams conference links in the active tab of Chrome, Edge, Brave, Arc or Safari when the browser uses your microphone.
                macOS may ask you to allow browser automation. Firefox is not supported.
              </p>
            </div>
            <Switch id="detect-browser-calls" aria-describedby="browser-call-description" checked={settings.preferences.browser_calls} onCheckedChange={browser_calls => save({ browser_calls })} disabled={saving || !settings.preferences.enabled} />
          </div>
          <div className="flex items-start justify-between gap-4">
            <div>
              <label htmlFor="stop-after-call" className="font-medium">Stop recording when the call ends</label>
              <p id="stop-call-description" className="text-sm text-gray-600 max-w-prose">
                Waits one minute without call activity before stopping and saving. Applies only to recordings started by call detection.
                If you stop a recording yourself, it stays stopped for the rest of that call.
              </p>
            </div>
            <Switch id="stop-after-call" aria-describedby="stop-call-description" checked={settings.preferences.stop_when_call_ends} onCheckedChange={stop_when_call_ends => save({ stop_when_call_ends })} disabled={saving || !settings.preferences.enabled} />
          </div>
          <p className="text-sm text-gray-600">Microphone previews can resemble calls. Muting or switching audio devices may interrupt detection. You can stop recording at any time. Turning detection off leaves the current recording running.</p>
        </>
      )}
    </section>
  );
}
