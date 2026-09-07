import React, { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { RefreshCw, Mic, Speaker } from 'lucide-react';
import { AudioLevelMeter, CompactAudioLevelMeter } from './AudioLevelMeter';
import { AudioBackendSelector } from './AudioBackendSelector';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import Analytics from '@/lib/analytics';

export interface AudioDevice {
  name: string;
  device_type: 'Input' | 'Output';
}

export interface SelectedDevices {
  micDevice: string | null;
  systemDevice: string | null;
}

export interface AudioLevelData {
  device_name: string;
  device_type: string;
  rms_level: number;
  peak_level: number;
  is_active: boolean;
}

export interface AudioLevelUpdate {
  timestamp: number;
  levels: AudioLevelData[];
}

interface DeviceSelectionProps {
  selectedDevices: SelectedDevices;
  onDeviceChange: (devices: SelectedDevices) => void;
  disabled?: boolean;
}

export function DeviceSelection({ selectedDevices, onDeviceChange, disabled = false }: DeviceSelectionProps) {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [audioLevels, setAudioLevels] = useState<Map<string, AudioLevelData>>(new Map());
  const [isMonitoring, setIsMonitoring] = useState(false);

  // Filter devices by type
  const inputDevices = devices.filter(device => device.device_type === 'Input');
  const outputDevices = devices.filter(device => device.device_type === 'Output');

  // Fetch available audio devices
  const fetchDevices = async () => {
    try {
      setError(null);
      const result = await invoke<AudioDevice[]>('get_audio_devices');
      setDevices(result);
      console.log('Fetched audio devices:', result);
    } catch (err) {
      console.error('Failed to fetch audio devices:', err);
      setError('Failed to load audio devices. Please check your system audio settings.');
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  // Load devices on component mount
  useEffect(() => {
    fetchDevices();
  }, []);

  // Auto-select the default when a previously-selected device is no longer
  // available (e.g. AirPods that have since disconnected). Without this the
  // dropdown keeps showing a device that's gone — Radix Select renders an
  // empty/stale trigger for a value not among its options — and start would
  // send a name the backend can't open. Matches Pro's DeviceSelection prune.
  //
  // Format trap: selected values carry the " (input)"/" (output)" suffix (the
  // SelectItem value form below), while enumeration returns raw device names —
  // so compare against the suffixed form, else every non-default selection is
  // wrongly pruned.
  //
  // ponytail: enumeration-driven prune only (fires on mount + manual refresh).
  // We deliberately do NOT mutate the selection from the mic-device-switched
  // event — that stays toast-only; reopening Settings after a meeting refetches
  // and prunes. onDeviceChange is held in a ref so the effect isn't re-run by
  // the parent passing a fresh (non-memoized) handler each render.
  const onDeviceChangeRef = useRef(onDeviceChange);
  useEffect(() => { onDeviceChangeRef.current = onDeviceChange; }, [onDeviceChange]);

  useEffect(() => {
    // Skip while loading (list not ready) or disabled (selector locked during
    // recording — never reset the device a live recording is using).
    if (loading || disabled) return;

    const inputs = devices.filter(d => d.device_type === 'Input');
    const outputs = devices.filter(d => d.device_type === 'Output');

    let next = selectedDevices;
    // Guard on a non-empty list per direction so a transient phantom-empty
    // enumeration doesn't wipe a valid selection.
    if (
      selectedDevices.micDevice &&
      inputs.length > 0 &&
      !inputs.some(d => `${d.name} (input)` === selectedDevices.micDevice)
    ) {
      console.warn(`[DeviceSelection] Selected mic '${selectedDevices.micDevice}' not available — resetting to default`);
      next = { ...next, micDevice: null };
    }
    if (
      selectedDevices.systemDevice &&
      outputs.length > 0 &&
      !outputs.some(d => `${d.name} (output)` === selectedDevices.systemDevice)
    ) {
      console.warn(`[DeviceSelection] Selected system audio '${selectedDevices.systemDevice}' not available — resetting to default`);
      next = { ...next, systemDevice: null };
    }

    // Only propagate if something actually changed. Resetting to null makes the
    // next run's absent-check falsy, so there's no loop.
    if (next !== selectedDevices) {
      onDeviceChangeRef.current(next);
    }
  }, [devices, loading, disabled, selectedDevices]);

  // Set up audio level event listener
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupAudioLevelListener = async () => {
      try {
        unlisten = await listen<AudioLevelUpdate>('audio-levels', (event) => {
          const levelUpdate = event.payload;
          const newLevels = new Map<string, AudioLevelData>();

          levelUpdate.levels.forEach(level => {
            newLevels.set(level.device_name, level);
          });

          setAudioLevels(newLevels);
        });
      } catch (err) {
        console.error('Failed to setup audio level listener:', err);
      }
    };

    setupAudioLevelListener();

    // Cleanup function
    return () => {
      if (unlisten) {
        unlisten();
      }
      // Stop monitoring when component unmounts
      if (isMonitoring) {
        stopAudioLevelMonitoring();
      }
    };
  }, [isMonitoring]);

  // Handle device refresh
  const handleRefresh = async () => {
    setRefreshing(true);
    await fetchDevices();
  };

  // Helper function to detect device category and Bluetooth status
  const getDeviceMetadata = (deviceName: string) => {
    const nameLower = deviceName.toLowerCase();

    // Detect if it's Bluetooth
    const isBluetooth = nameLower.includes('airpods')
      || nameLower.includes('bluetooth')
      || nameLower.includes('wireless')
      || nameLower.includes('wh-')  // Sony WH-* series
      || nameLower.includes('bt ');

    // Categorize device
    let category = 'wired';
    if (deviceName === 'default') {
      category = 'default';
    } else if (nameLower.includes('airpods')) {
      category = 'airpods';
    } else if (isBluetooth) {
      category = 'bluetooth';
    }

    return { isBluetooth, category };
  };

  // Handle microphone device selection
  const handleMicDeviceChange = (deviceName: string) => {
    const newDevices = {
      ...selectedDevices,
      micDevice: deviceName === 'default' ? null : deviceName
    };
    onDeviceChange(newDevices);

    // Track device selection analytics with enhanced metadata
    const metadata = getDeviceMetadata(deviceName);
    Analytics.track('microphone_selected', {
      device_category: metadata.category,
      is_bluetooth: metadata.isBluetooth.toString(),
      has_system_audio: (!!selectedDevices.systemDevice).toString()
    }).catch(err => console.error('Failed to track microphone selection:', err));
  };

  // Handle system audio device selection
  const handleSystemDeviceChange = (deviceName: string) => {
    const newDevices = {
      ...selectedDevices,
      systemDevice: deviceName === 'default' ? null : deviceName
    };
    onDeviceChange(newDevices);

    // Track device selection analytics with enhanced metadata
    const metadata = getDeviceMetadata(deviceName);
    Analytics.track('system_audio_selected', {
      device_category: metadata.category,
      is_bluetooth: metadata.isBluetooth.toString(),
      has_microphone: (!!selectedDevices.micDevice).toString()
    }).catch(err => console.error('Failed to track system audio selection:', err));
  };

  // Start audio level monitoring
  const startAudioLevelMonitoring = async () => {
    try {
      // Only monitor input devices for now (microphones)
      const deviceNames = inputDevices.map(device => device.name);
      if (deviceNames.length === 0) {
        setError('No microphone devices found to monitor');
        return;
      }

      await invoke('start_audio_level_monitoring', { deviceNames });
      setIsMonitoring(true);
      console.log('Started audio level monitoring for input devices:', deviceNames);
    } catch (err) {
      console.error('Failed to start audio level monitoring:', err);
      setError('Failed to start audio level monitoring');
    }
  };

  // Stop audio level monitoring
  const stopAudioLevelMonitoring = async () => {
    try {
      await invoke('stop_audio_level_monitoring');
      setIsMonitoring(false);
      setAudioLevels(new Map());
      console.log('Stopped audio level monitoring');
    } catch (err) {
      console.error('Failed to stop audio level monitoring:', err);
    }
  };

  // Toggle audio level monitoring
  const toggleAudioLevelMonitoring = async () => {
    if (isMonitoring) {
      await stopAudioLevelMonitoring();
    } else {
      await startAudioLevelMonitoring();
    }
  };

  if (loading) {
    return (
      <div className="p-4 space-y-4">
        <div className="animate-pulse">
          <div className="h-4 bg-gray-200 rounded w-1/3 mb-4"></div>
          <div className="h-10 bg-gray-200 rounded mb-3"></div>
          <div className="h-10 bg-gray-200 rounded"></div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium text-gray-900">Audio Devices</h4>
        <div className="flex items-center space-x-2">
          <button
            onClick={toggleAudioLevelMonitoring}
            disabled={disabled || inputDevices.length === 0}
            className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors border ${
              isMonitoring
                ? 'bg-red-500 text-white hover:bg-red-600 border-red-500'
                : 'bg-blue-500 text-white hover:bg-blue-600 border-blue-500'
            } disabled:pointer-events-none disabled:opacity-50`}
            title={inputDevices.length === 0 ? 'No microphones available to test' : 'Test your audio devices'}
          >
            {isMonitoring ? 'Stop Test' : 'Test Mic'}
          </button>
          <button
            onClick={handleRefresh}
            disabled={refreshing || disabled}
            className="h-8 w-8 p-0 inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors hover:bg-gray-100 disabled:pointer-events-none disabled:opacity-50"
          >
            <RefreshCw className={`h-4 w-4 ${refreshing ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>

      {error && (
        <div className="p-3 text-sm text-red-700 bg-red-50 border border-red-200 rounded-md">
          {error}
        </div>
      )}

      <div className="space-y-3">
        {/* Microphone Selection */}
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Mic className="h-4 w-4 text-gray-600" />
            <Label htmlFor="mic-selection" className="text-sm font-medium text-gray-700">
              Microphone
            </Label>
          </div>
          <Select
            value={selectedDevices.micDevice || 'default'}
            onValueChange={handleMicDeviceChange}
            disabled={disabled}
          >
            <SelectTrigger id="mic-selection" className="w-full">
              <SelectValue placeholder="Select Microphone" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default">Default Microphone</SelectItem>
              {inputDevices.map((device) => (
                <SelectItem
                  key={device.name}
                  value={`${device.name} (${device.device_type.toLowerCase()})`}
                >
                  {device.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {inputDevices.length === 0 && (
            <p className="text-xs text-gray-500">No microphone devices found</p>
          )}

          {/* Audio Level Meters for Input Devices */}
          {isMonitoring && inputDevices.length > 0 && (
            <div className="space-y-2 pt-2 border-t border-gray-100">
              <p className="text-xs text-gray-600 font-medium">Microphone Levels:</p>
              {inputDevices.map((device) => {
                const levelData = audioLevels.get(device.name);
                return (
                  <div key={`level-${device.name}`} className="space-y-1">
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-gray-600 truncate max-w-[200px]">
                        {device.name}
                      </span>
                      {levelData && (
                        <CompactAudioLevelMeter
                          rmsLevel={levelData.rms_level}
                          peakLevel={levelData.peak_level}
                          isActive={levelData.is_active}
                        />
                      )}
                    </div>
                    {levelData && (
                      <AudioLevelMeter
                        rmsLevel={levelData.rms_level}
                        peakLevel={levelData.peak_level}
                        isActive={levelData.is_active}
                        deviceName={device.name}
                        size="small"
                      />
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* System Audio Selection */}
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Speaker className="h-4 w-4 text-gray-600" />
            <Label htmlFor="system-selection" className="text-sm font-medium text-gray-700">
              System Audio
            </Label>
          </div>

          <Select
            value={selectedDevices.systemDevice || 'default'}
            onValueChange={handleSystemDeviceChange}
            disabled={disabled}
          >
            <SelectTrigger id="system-selection" className="w-full">
              <SelectValue placeholder="Select System Audio" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default">Default System Audio</SelectItem>
              {outputDevices.map((device) => (
                <SelectItem
                  key={device.name}
                  value={`${device.name} (${device.device_type.toLowerCase()})`}
                >
                  {device.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {outputDevices.length === 0 && (
            <p className="text-xs text-gray-500">No system audio devices found</p>
          )}

          {/* System audio info */}
          <div className="mt-2 p-2 bg-gray-50 rounded-md border border-gray-200">
            <div className="flex items-center gap-2">
              <div className="w-2 h-2 rounded-full bg-gray-300" />
              <div className="flex-1 h-2 bg-gray-200 rounded-sm overflow-hidden" />
              <span className="text-xs text-gray-400 font-mono min-w-[3rem] text-right">--</span>
            </div>
            <p className="text-xs text-gray-400 mt-1.5 text-center">
              System audio levels are captured during recording
            </p>
          </div>

          {/* Backend Selection - available on all platforms */}
          {!disabled && (
            <div className="pt-3 border-t border-gray-100">
              <AudioBackendSelector disabled={disabled} />
            </div>
          )}
        </div>
      </div>

      {/* Info text */}
      <div className="text-xs text-gray-500 space-y-1">
        <p>• <strong>Microphone:</strong> Records your voice and ambient sound</p>
        <p>• <strong>System Audio:</strong> Records computer audio (music, calls, etc.)</p>
        {isMonitoring && (
          <p>• <strong>Mic Levels:</strong> Green = good, Yellow = loud, Red = too loud</p>
        )}
        {!isMonitoring && inputDevices.length > 0 && (
          <p>• <strong>Tip:</strong> Click "Test Mic" to check if your microphone is working</p>
        )}
      </div>
    </div>
  );
}
