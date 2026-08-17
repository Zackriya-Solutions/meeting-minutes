'use client';

import React, { useCallback, useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { CalendarDays, CheckCircle2, ExternalLink, Play, RefreshCw, XCircle } from 'lucide-react';
import { toast } from 'sonner';
import { Switch } from '@/components/ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Input } from '@/components/ui/input';

interface CalendarAccountStatus {
  connected: boolean;
  email: string | null;
  status: string; // "connected" | "needs_reauth" | "disconnected"
}

interface AutoStartSettings {
  enabled: boolean;
  mode: string; // "ask" | "silent"
  graceMinutes: number;
}

interface CalendarEventDto {
  id: string;
  title: string;
  start_time: string;
  end_time: string;
  meeting_url: string | null;
  meeting_provider: string | null;
  is_meeting: boolean;
}

export function CalendarSettings() {
  const router = useRouter();
  const [status, setStatus] = useState<CalendarAccountStatus>({ connected: false, email: null, status: 'disconnected' });
  const [events, setEvents] = useState<CalendarEventDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [connecting, setConnecting] = useState(false);
  const [autoStart, setAutoStart] = useState<AutoStartSettings>({ enabled: false, mode: 'ask', graceMinutes: 5 });
  const [savingAutoStart, setSavingAutoStart] = useState(false);

  const loadStatus = useCallback(async () => {
    try {
      const result = await invoke<CalendarAccountStatus>('calendar_get_status');
      setStatus(result);
    } catch (error) {
      console.error('Failed to load calendar status:', error);
    }
  }, []);

  const loadEvents = useCallback(async () => {
    try {
      const result = await invoke<CalendarEventDto[]>('calendar_get_upcoming_events');
      setEvents(result);
    } catch (error) {
      console.error('Failed to load upcoming calendar events:', error);
    }
  }, []);

  const loadAutoStart = useCallback(async () => {
    try {
      const result = await invoke<AutoStartSettings>('calendar_get_auto_start_settings');
      setAutoStart(result);
    } catch (error) {
      console.error('Failed to load auto-start settings:', error);
    }
  }, []);

  useEffect(() => {
    const load = async () => {
      setLoading(true);
      await loadStatus();
      await loadEvents();
      await loadAutoStart();
      setLoading(false);
    };
    load();
  }, [loadStatus, loadEvents, loadAutoStart]);

  const saveAutoStart = async (next: AutoStartSettings) => {
    setAutoStart(next);
    setSavingAutoStart(true);
    try {
      await invoke('calendar_update_auto_start_settings', { settings: next });
    } catch (error) {
      console.error('Failed to save auto-start settings:', error);
      toast.error('Failed to save auto-start settings');
    } finally {
      setSavingAutoStart(false);
    }
  };

  const handleConnect = async () => {
    setConnecting(true);
    try {
      toast.info('Opening your browser to sign in with Google…', { duration: 4000 });
      const result = await invoke<CalendarAccountStatus>('calendar_connect');
      setStatus(result);
      toast.success(`Google Calendar connected as ${result.email}`);
      await loadEvents();
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
      setStatus({ connected: false, email: null, status: 'disconnected' });
      setEvents([]);
      toast.success('Google Calendar disconnected');
    } catch (error) {
      console.error('Failed to disconnect Google Calendar:', error);
      toast.error('Failed to disconnect Google Calendar');
    }
  };

  const handleRecordNow = (event: CalendarEventDto) => {
    // Reuses the same auto-start mechanism as the tray "Start Recording" action
    // (sessionStorage flag picked up by useRecordingStart on the main page).
    sessionStorage.setItem('autoStartRecording', 'true');
    sessionStorage.setItem('autoStartMeetingTitle', event.title);
    router.push('/');
  };

  const formatTime = (iso: string) =>
    new Date(iso).toLocaleString(undefined, {
      weekday: 'short',
      hour: 'numeric',
      minute: '2-digit',
      month: 'short',
      day: 'numeric',
    });

  if (loading) {
    return <div className="text-sm text-gray-500 p-6">Loading calendar settings…</div>;
  }

  return (
    <div className="space-y-6">
      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <CalendarDays className="h-5 w-5 text-gray-600" />
            <div>
              <h3 className="text-lg font-semibold text-gray-900">Google Calendar</h3>
              {status.connected ? (
                <p className="text-sm text-gray-600 flex items-center gap-1.5 mt-0.5">
                  <CheckCircle2 className="h-4 w-4 text-green-600" />
                  Connected as {status.email}
                </p>
              ) : status.status === 'needs_reauth' ? (
                <p className="text-sm text-amber-700 flex items-center gap-1.5 mt-0.5">
                  <XCircle className="h-4 w-4" />
                  Access expired for {status.email} — reconnect below
                </p>
              ) : (
                <p className="text-sm text-gray-500 mt-0.5">Not connected</p>
              )}
            </div>
          </div>

          {status.connected ? (
            <button
              onClick={handleDisconnect}
              className="px-4 py-2 text-sm font-medium text-gray-700 border border-gray-300 rounded-md hover:bg-gray-50 transition-colors"
            >
              Disconnect
            </button>
          ) : (
            <button
              onClick={handleConnect}
              disabled={connecting}
              className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 disabled:opacity-60 transition-colors flex items-center gap-2"
            >
              {connecting ? <RefreshCw className="h-4 w-4 animate-spin" /> : <ExternalLink className="h-4 w-4" />}
              {connecting ? 'Waiting for sign-in…' : 'Connect Google Calendar'}
            </button>
          )}
        </div>

        <p className="text-xs text-gray-500 mt-4">
          Read-only access to your primary calendar. Meetily only uses this to see event times, titles, and
          conferencing links (Google Meet, Zoom, Teams) — it never edits your calendar.
        </p>
      </div>

      {status.connected && (
        <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-base font-semibold text-gray-900">Auto-start recording</h3>
              <p className="text-sm text-gray-500 mt-0.5">
                Automatically start recording when a synced meeting begins, and stop it after it ends.
              </p>
            </div>
            <Switch
              checked={autoStart.enabled}
              onCheckedChange={(enabled) => saveAutoStart({ ...autoStart, enabled })}
              disabled={savingAutoStart}
            />
          </div>

          {autoStart.enabled && (
            <div className="grid grid-cols-2 gap-4 pt-2 border-t border-gray-100">
              <div>
                <label className="text-sm font-medium text-gray-700 block mb-1.5">Before starting</label>
                <Select
                  value={autoStart.mode}
                  onValueChange={(mode) => saveAutoStart({ ...autoStart, mode })}
                  disabled={savingAutoStart}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="ask">Ask first (45s to cancel)</SelectItem>
                    <SelectItem value="silent">Start silently</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div>
                <label className="text-sm font-medium text-gray-700 block mb-1.5">Stop after event ends</label>
                <div className="flex items-center gap-2">
                  <Input
                    type="number"
                    min={0}
                    max={60}
                    value={autoStart.graceMinutes}
                    onChange={(e) => {
                      const graceMinutes = Math.max(0, Math.min(60, Number(e.target.value) || 0));
                      setAutoStart({ ...autoStart, graceMinutes });
                    }}
                    onBlur={() => saveAutoStart(autoStart)}
                    disabled={savingAutoStart}
                    className="w-20"
                  />
                  <span className="text-sm text-gray-500">minutes</span>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {status.connected && (
        <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-base font-semibold text-gray-900">Upcoming meetings</h3>
            <button
              onClick={loadEvents}
              className="text-sm text-gray-500 hover:text-gray-700 flex items-center gap-1"
            >
              <RefreshCw className="h-3.5 w-3.5" />
              Refresh
            </button>
          </div>

          {events.length === 0 ? (
            <p className="text-sm text-gray-500">No meetings synced in the next 24 hours yet.</p>
          ) : (
            <ul className="divide-y divide-gray-100">
              {events.map((event) => (
                <li key={event.id} className="py-3 flex items-center justify-between gap-4">
                  <div className="min-w-0">
                    <p className="text-sm font-medium text-gray-900 truncate">{event.title}</p>
                    <p className="text-xs text-gray-500">
                      {formatTime(event.start_time)}
                      {event.meeting_provider ? ` · ${event.meeting_provider}` : ''}
                    </p>
                  </div>
                  {event.is_meeting && (
                    <button
                      onClick={() => handleRecordNow(event)}
                      className="shrink-0 px-3 py-1.5 text-xs font-medium text-blue-700 border border-blue-200 rounded-md hover:bg-blue-50 transition-colors flex items-center gap-1.5"
                    >
                      <Play className="h-3 w-3" />
                      Record now
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

    </div>
  );
}
