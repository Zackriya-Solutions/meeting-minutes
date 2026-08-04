'use client';

import React, { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { type } from '@tauri-apps/plugin-os';
import { toast } from 'sonner';
import { Keyboard, AlertTriangle, Check, RotateCcw, Edit2, X } from 'lucide-react';
import { validateShortcut, formatShortcut, DEFAULT_RECORDING_SHORTCUT } from '@/lib/shortcutUtils';

export function ShortcutSettings() {
  const [currentShortcut, setCurrentShortcut] = useState<string>(DEFAULT_RECORDING_SHORTCUT);
  const [hasPermission, setHasPermission] = useState<boolean>(true);
  const [isMacOS, setIsMacOS] = useState<boolean>(false);
  const [isCapturing, setIsCapturing] = useState<boolean>(false);
  const [capturedKeys, setCapturedKeys] = useState<string>('');
  const [isSaving, setIsSaving] = useState<boolean>(false);
  const captureRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const loadData = async () => {
      try {
        const os = await type();
        setIsMacOS(os === 'macos');
      } catch {}

      try {
        const shortcut = await invoke<string>('get_recording_shortcut');
        setCurrentShortcut(shortcut);
      } catch (e) {
        console.error('Failed to load shortcut:', e);
      }

      try {
        const permitted = await invoke<boolean>('check_shortcut_permission');
        setHasPermission(permitted);
      } catch {
        setHasPermission(true);
      }
    };
    loadData();
  }, []);

  const openAccessibilitySettings = useCallback(async () => {
    try {
      await invoke('open_url', {
        url: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility',
      });
    } catch {
      try {
        await invoke('open_external_url', {
          url: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility',
        });
      } catch (e) {
        console.error('Failed to open accessibility settings:', e);
        toast.error('Please open System Preferences > Privacy & Security > Accessibility manually.');
      }
    }
  }, []);

  const startCapture = useCallback(() => {
    setIsCapturing(true);
    setCapturedKeys('');
    setTimeout(() => captureRef.current?.focus(), 50);
  }, []);

  const cancelCapture = useCallback(() => {
    setIsCapturing(false);
    setCapturedKeys('');
  }, []);

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();

    const modifiers: string[] = [];
    if (e.ctrlKey || e.key === 'Control') modifiers.push('Control');
    if (e.shiftKey && e.key !== 'Shift') modifiers.push('Shift');
    if (e.altKey && e.key !== 'Alt') modifiers.push('Alt');
    if (e.metaKey && e.key !== 'Meta') modifiers.push('Meta');

    const nonModifierKeys = new Set([
      'Control', 'Shift', 'Alt', 'Meta', 'CapsLock', 'Tab',
      'Escape', 'ContextMenu',
    ]);

    if (e.key === 'Escape') {
      cancelCapture();
      return;
    }

    if (!nonModifierKeys.has(e.key) && modifiers.length > 0) {
      const combo = [...modifiers, e.key].join('+');
      setCapturedKeys(combo);
    } else if (modifiers.length > 0) {
      setCapturedKeys(modifiers.join('+') + '+...');
    }
  }, [cancelCapture]);

  const confirmShortcut = useCallback(async () => {
    if (!capturedKeys || capturedKeys.endsWith('+...')) {
      toast.error('Please press a non-modifier key to complete the shortcut.');
      return;
    }
    if (!validateShortcut(capturedKeys)) {
      toast.error('Invalid shortcut. Include at least one modifier key (Ctrl, Shift, Alt) and one key.');
      return;
    }
    setIsSaving(true);
    try {
      await invoke('set_recording_shortcut', { shortcut: capturedKeys });
      setCurrentShortcut(capturedKeys);
      setIsCapturing(false);
      setCapturedKeys('');
      toast.success(`Shortcut updated to ${formatShortcut(capturedKeys)}`);
    } catch (e) {
      toast.error(`Failed to set shortcut: ${e}`);
    } finally {
      setIsSaving(false);
    }
  }, [capturedKeys]);

  const resetToDefault = useCallback(async () => {
    setIsSaving(true);
    try {
      await invoke('set_recording_shortcut', { shortcut: DEFAULT_RECORDING_SHORTCUT });
      setCurrentShortcut(DEFAULT_RECORDING_SHORTCUT);
      toast.success(`Shortcut reset to ${formatShortcut(DEFAULT_RECORDING_SHORTCUT)}`);
    } catch (e) {
      toast.error(`Failed to reset shortcut: ${e}`);
    } finally {
      setIsSaving(false);
    }
  }, []);

  return (
    <div className="space-y-6">
      <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
        <div className="flex items-center gap-3 mb-5">
          <div className="p-2 bg-blue-50 rounded-lg">
            <Keyboard className="w-5 h-5 text-blue-600" />
          </div>
          <div>
            <h2 className="text-lg font-semibold text-gray-900">Keyboard Shortcuts</h2>
            <p className="text-sm text-gray-500">Configure global shortcuts that work even when the app is in the background.</p>
          </div>
        </div>

        {isMacOS && !hasPermission && (
          <div className="mb-4 flex items-start gap-3 p-4 bg-amber-50 border border-amber-200 rounded-lg">
            <AlertTriangle className="w-5 h-5 text-amber-600 mt-0.5 shrink-0" />
            <div className="flex-1">
              <p className="text-sm font-medium text-amber-800">Accessibility Permission Required</p>
              <p className="text-sm text-amber-700 mt-1">
                Global shortcuts on macOS require Accessibility permission. Grant it in System Settings to enable this feature.
              </p>
              <button
                onClick={openAccessibilitySettings}
                className="mt-2 text-sm font-medium text-amber-800 underline hover:text-amber-900"
              >
                Open Accessibility Settings →
              </button>
            </div>
          </div>
        )}

        {isMacOS && hasPermission && (
          <div className="mb-4 flex items-center gap-2 p-3 bg-green-50 border border-green-200 rounded-lg">
            <Check className="w-4 h-4 text-green-600 shrink-0" />
            <p className="text-sm text-green-700">Accessibility permission granted. Global shortcuts are active.</p>
          </div>
        )}

        <div className="space-y-4">
          <div className="flex items-center justify-between p-4 border border-gray-200 rounded-lg bg-gray-50">
            <div>
              <p className="text-sm font-medium text-gray-900">Toggle Recording</p>
              <p className="text-xs text-gray-500 mt-0.5">Start or stop recording from anywhere</p>
            </div>

            <div className="flex items-center gap-2">
              {!isCapturing ? (
                <>
                  <span className="inline-flex items-center px-3 py-1.5 bg-white border border-gray-300 rounded-md text-sm font-mono font-medium text-gray-800 shadow-sm">
                    {formatShortcut(currentShortcut)}
                  </span>
                  <button
                    onClick={startCapture}
                    disabled={isSaving}
                    className="flex items-center gap-1.5 px-3 py-1.5 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors disabled:opacity-50"
                  >
                    <Edit2 className="w-3.5 h-3.5" />
                    Change
                  </button>
                  {currentShortcut !== DEFAULT_RECORDING_SHORTCUT && (
                    <button
                      onClick={resetToDefault}
                      disabled={isSaving}
                      className="flex items-center gap-1.5 px-3 py-1.5 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors disabled:opacity-50"
                      title={`Reset to default (${formatShortcut(DEFAULT_RECORDING_SHORTCUT)})`}
                    >
                      <RotateCcw className="w-3.5 h-3.5" />
                      Reset
                    </button>
                  )}
                </>
              ) : (
                <div
                  ref={captureRef}
                  tabIndex={0}
                  onKeyDown={handleKeyDown}
                  className="flex items-center gap-2 outline-none"
                >
                  <div className="flex items-center gap-2 px-3 py-1.5 bg-blue-50 border-2 border-blue-400 rounded-md min-w-[140px] text-sm font-mono">
                    {capturedKeys && !capturedKeys.endsWith('+...')
                      ? formatShortcut(capturedKeys)
                      : capturedKeys || (
                          <span className="text-gray-400 animate-pulse">Press keys…</span>
                        )}
                  </div>
                  <button
                    onClick={confirmShortcut}
                    disabled={!capturedKeys || capturedKeys.endsWith('+...') || isSaving}
                    className="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-blue-600 text-white rounded-md hover:bg-blue-700 transition-colors disabled:opacity-40"
                  >
                    <Check className="w-3.5 h-3.5" />
                    Save
                  </button>
                  <button
                    onClick={cancelCapture}
                    className="flex items-center gap-1.5 px-3 py-1.5 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors"
                  >
                    <X className="w-3.5 h-3.5" />
                    Cancel
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>

        <p className="mt-4 text-xs text-gray-500">
          Default shortcut: <span className="font-mono font-medium">{formatShortcut(DEFAULT_RECORDING_SHORTCUT)}</span>
        </p>
      </div>
    </div>
  );
}
