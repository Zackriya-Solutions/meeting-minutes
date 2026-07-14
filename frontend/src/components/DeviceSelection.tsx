import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { RefreshCw, Mic, Speaker } from '@/components/memento/LucideCompat';
import { AudioLevelMeter, CompactAudioLevelMeter } from './AudioLevelMeter';
import { AudioBackendSelector } from './AudioBackendSelector';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import Analytics from '@/lib/analytics';
import { useT } from '@/lib/i18n';

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
  const t = useT();
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [audioLevels, setAudioLevels] = useState<Map<string, AudioLevelData>>(new Map());
  const [isMonitoring, setIsMonitoring] = useState(false);
  const [showLevels, setShowLevels] = useState(false);

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
      setError(t('Failed to load audio devices. Please check your system audio settings.'));
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  // Load devices on component mount
  useEffect(() => {
    fetchDevices();
  }, []);

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
        setError(t('No microphone devices found to monitor'));
        return;
      }

      await invoke('start_audio_level_monitoring', { deviceNames });
      setIsMonitoring(true);
      setShowLevels(true);
      console.log('Started audio level monitoring for input devices:', deviceNames);
    } catch (err) {
      console.error('Failed to start audio level monitoring:', err);
      setError(t('Failed to start audio level monitoring'));
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
          <div className="h-4 bg-[var(--bg-elevated)] rounded w-1/3 mb-4"></div>
          <div className="h-10 bg-[var(--bg-elevated)] rounded mb-3"></div>
          <div className="h-10 bg-[var(--bg-elevated)] rounded"></div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium text-[var(--fg1)]">{t('Audio Devices')}</h4>
        <div className="flex items-center space-x-2">
          {/* TODO: Monitoring */}
          {/* <button */}
          {/*   onClick={toggleAudioLevelMonitoring} */}
          {/*   disabled={disabled || inputDevices.length === 0} */}
          {/*   className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${ */}
          {/*     isMonitoring */}
          {/*       ? 'bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] text-[var(--danger)] hover:bg-[color-mix(in_srgb,var(--danger)_12%,transparent)]' */}
          {/*       : 'bg-[color-mix(in_srgb,var(--success)_12%,transparent)] text-[var(--success)] hover:bg-[color-mix(in_srgb,var(--success)_12%,transparent)]' */}
          {/*   } disabled:pointer-events-none disabled:opacity-50`} */}
          {/*   title={inputDevices.length === 0 ? 'No microphones available to test' : ''} */}
          {/* > */}
          {/*   {isMonitoring ? 'Stop Test' : 'Test Mic'} */}
          {/* </button> */}
          <button
            onClick={handleRefresh}
            disabled={refreshing || disabled}
            className="h-8 w-8 p-0 inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors hover:bg-[var(--bg-elevated)] disabled:pointer-events-none disabled:opacity-50"
          >
            <RefreshCw className={`h-4 w-4 ${refreshing ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>

      {error && (
        <div className="p-3 text-sm text-[var(--danger)] bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] border border-[color-mix(in_srgb,var(--danger)_42%,transparent)] rounded-md">
          {error}
        </div>
      )}

      <div className="space-y-3">
        {/* Microphone Selection */}
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <Mic className="h-4 w-4 text-[var(--fg2)]" />
            <Label htmlFor="mic-selection" className="text-sm font-medium text-[var(--fg2)]">
              {t('Microphone')}
            </Label>
          </div>
          <Select
            value={selectedDevices.micDevice || 'default'}
            onValueChange={handleMicDeviceChange}
            disabled={disabled}
          >
            <SelectTrigger id="mic-selection" className="w-full">
              <SelectValue placeholder={t('Select Microphone')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default">{t('Default Microphone')}</SelectItem>
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
            <p className="text-xs text-[var(--fg2)]">{t('No microphone devices found')}</p>
          )}

          {/* Audio Level Meters for Input Devices */}
          {showLevels && inputDevices.length > 0 && (
            <div className="space-y-2 pt-2 border-t border-[var(--border-subtle)]">
              <p className="text-xs text-[var(--fg2)] font-medium">{t('Microphone Levels:')}</p>
              {inputDevices.map((device) => {
                const levelData = audioLevels.get(device.name);
                return (
                  <div key={`level-${device.name}`} className="space-y-1">
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-[var(--fg2)] truncate max-w-[200px]">
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
            <Speaker className="h-4 w-4 text-[var(--fg2)]" />
            <Label htmlFor="system-selection" className="text-sm font-medium text-[var(--fg2)]">
              {t('System Audio')}
            </Label>
          </div>

          <Select
            value={selectedDevices.systemDevice || 'default'}
            onValueChange={handleSystemDeviceChange}
            disabled={disabled}
          >
            <SelectTrigger id="system-selection" className="w-full">
              <SelectValue placeholder={t('Select System Audio')} />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="default">{t('Default System Audio')}</SelectItem>
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
            <p className="text-xs text-[var(--fg2)]">{t('No system audio devices found')}</p>
          )}

          {/* Backend Selection - available on all platforms */}
          {!disabled && (
            <div className="pt-3 border-t border-[var(--border-subtle)]">
              <AudioBackendSelector disabled={disabled} />
            </div>
          )}
        </div>
      </div>

      {/* Info text */}
      <div className="text-xs text-[var(--fg2)] space-y-1">
        <p>• <strong>{t('Microphone:')}</strong> {t('Records your voice and ambient sound')}</p>
        <p>• <strong>{t('System Audio:')}</strong> {t('Records computer audio (music, calls, etc.)')}</p>
        {isMonitoring && (
          <p>• <strong>{t('Mic Levels:')}</strong> {t('Green = good, Yellow = loud, Red = too loud')}</p>
        )}
        {!isMonitoring && inputDevices.length > 0 && (
          <p>• <strong>{t('Tip:')}</strong> {t('Click "Test Mic" to check if your microphone is working')}</p>
        )}
      </div>
    </div>
  );
}
