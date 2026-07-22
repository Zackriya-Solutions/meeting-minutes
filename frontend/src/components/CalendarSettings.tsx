import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { CalendarDays, ShieldCheck } from 'lucide-react';
import { Switch } from '@/components/ui/switch';
import { Input } from '@/components/ui/input';

interface CalendarConnectionStatus {
  connected: boolean;
  scope: string | null;
}

export function CalendarSettings() {
  const [loading, setLoading] = useState(true);
  const [connecting, setConnecting] = useState(false);
  const [status, setStatus] = useState<CalendarConnectionStatus>({ connected: false, scope: null });
  const [autoDetect, setAutoDetect] = useState(false);
  const [clientId, setClientId] = useState('');
  const [clientSecret, setClientSecret] = useState('');

  useEffect(() => {
    const load = async () => {
      try {
        const [connectionStatus, autoDetectEnabled] = await Promise.all([
          invoke<CalendarConnectionStatus>('calendar_get_connection_status'),
          invoke<boolean>('calendar_get_auto_detect_enabled'),
        ]);
        setStatus(connectionStatus);
        setAutoDetect(autoDetectEnabled);
      } catch (error) {
        console.error('Failed to load Google Calendar settings:', error);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, []);

  const handleConnect = async () => {
    if (!clientId.trim() || !clientSecret.trim()) {
      toast.error('Client ID and Client Secret are required');
      return;
    }

    setConnecting(true);
    try {
      const result = await invoke<CalendarConnectionStatus>('calendar_start_oauth', {
        clientId: clientId.trim(),
        clientSecret: clientSecret.trim(),
      });
      setStatus(result);
      setClientSecret('');
      toast.success('Google Calendar connected');
    } catch (error) {
      console.error('Failed to connect Google Calendar:', error);
      toast.error('Failed to connect Google Calendar', {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setConnecting(false);
    }
  };

  const handleDisconnect = async () => {
    try {
      await invoke('calendar_disconnect');
      setStatus({ connected: false, scope: null });
      toast.success('Google Calendar disconnected');
    } catch (error) {
      console.error('Failed to disconnect Google Calendar:', error);
      toast.error('Failed to disconnect Google Calendar');
    }
  };

  const handleAutoDetectToggle = async (enabled: boolean) => {
    if (enabled) {
      try {
        const hasPermission = await invoke<boolean>('check_window_title_read_permission_command');
        if (!hasPermission) {
          const granted = await invoke<boolean>('request_window_title_read_permission_command');
          if (!granted) {
            toast.error('Screen Recording permission is required', {
              description: 'Meetily needs it to read browser window titles and detect Google Meet calls. Grant it in System Settings, then try again.',
            });
            return;
          }
        }
      } catch (error) {
        console.error('Failed to check/request Screen Recording permission:', error);
        toast.error('Could not verify Screen Recording permission');
        return;
      }
    }

    setAutoDetect(enabled);
    try {
      await invoke('calendar_toggle_auto_detect', { enabled });
    } catch (error) {
      console.error('Failed to update auto-detect setting:', error);
      setAutoDetect(!enabled);
      toast.error('Failed to update setting');
    }
  };

  if (loading) {
    return (
      <div className="animate-pulse">
        <div className="h-4 bg-gray-200 rounded w-1/4 mb-4"></div>
        <div className="h-8 bg-gray-200 rounded mb-4"></div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-semibold mb-4">Google Calendar</h3>
        <p className="text-sm text-gray-600 mb-6">
          Connect Google Calendar so meetings recorded around a scheduled Google Meet call are
          automatically tagged with the real title, attendees, and Meet link.
        </p>
      </div>

      {status.connected ? (
        <div className="flex items-center justify-between p-4 border rounded-lg bg-green-50">
          <div className="flex items-center gap-3">
            <ShieldCheck className="w-5 h-5 text-green-700" />
            <div>
              <div className="font-medium text-green-900">Connected</div>
              <div className="text-sm text-green-700">
                Meetily can read your calendar's Meet events (read-only access).
              </div>
            </div>
          </div>
          <button
            onClick={handleDisconnect}
            className="px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
          >
            Disconnect
          </button>
        </div>
      ) : (
        <div className="space-y-4 p-4 border rounded-lg">
          <div className="flex items-center gap-3">
            <CalendarDays className="w-5 h-5 text-gray-500" />
            <div className="font-medium">Not connected</div>
          </div>
          <p className="text-sm text-gray-600">
            Requires a Google Cloud project with the Calendar API enabled and an OAuth 2.0
            Client ID of type &quot;Desktop app&quot;. Paste those credentials below, then sign in
            when your browser opens.
          </p>
          <div className="space-y-2">
            <label className="text-sm font-medium text-gray-700">Client ID</label>
            <Input
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
              placeholder="xxxxxxxxxxxx.apps.googleusercontent.com"
              disabled={connecting}
            />
          </div>
          <div className="space-y-2">
            <label className="text-sm font-medium text-gray-700">Client Secret</label>
            <Input
              type="password"
              value={clientSecret}
              onChange={(e) => setClientSecret(e.target.value)}
              placeholder="Client secret"
              disabled={connecting}
            />
          </div>
          <button
            onClick={handleConnect}
            disabled={connecting}
            className="px-4 py-2 text-sm bg-blue-600 text-white rounded-md hover:bg-blue-700 transition-colors disabled:opacity-50"
          >
            {connecting ? 'Waiting for sign-in in your browser…' : 'Connect Google Calendar'}
          </button>
        </div>
      )}

      <div className="flex items-center justify-between p-4 border rounded-lg">
        <div className="flex-1">
          <div className="font-medium">Auto-detect Google Meet calls</div>
          <div className="text-sm text-gray-600">
            Automatically start recording when a Google Meet call is active in your browser, and
            stop shortly after it ends. macOS only, and requires granting Meetily Screen
            Recording permission to read browser window titles. Detection is heuristic
            (title-based) — it does not always catch every call.
          </div>
        </div>
        <Switch checked={autoDetect} onCheckedChange={handleAutoDetectToggle} />
      </div>
    </div>
  );
}
