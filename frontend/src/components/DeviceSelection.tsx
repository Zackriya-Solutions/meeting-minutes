import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Mic, Speaker } from '@/components/deslop-icons';
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/fluid-select';
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
    }
  };

  // Load devices on component mount
  useEffect(() => {
    fetchDevices();
  }, []);

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

  if (loading) {
    return (
      <div className="p-4 space-y-4">
        <div className="animate-pulse">
          <div className="h-4 bg-muted rounded w-1/3 mb-4"></div>
          <div className="h-10 bg-muted rounded mb-3"></div>
          <div className="h-10 bg-muted rounded"></div>
        </div>
      </div>
    );
  }

  return (
    <>
      {error && (
        <div className="p-3 text-sm text-destructive bg-destructive/10 border border-destructive/40 rounded-md">
          {error}
        </div>
      )}

      <section className="settings-section settings-cell">
        <div className="settings-cell__row">
          <span className="settings-cell__avatar" aria-hidden="true">
            <Mic size={20} />
          </span>
          <div className="settings-cell__text">
            <Label htmlFor="mic-selection" className="settings-cell__label">
              {t('Microphone')}
            </Label>
            <p className="settings-cell__caption">
              {inputDevices.length === 0
                ? t('No microphone devices found')
                : t('Voice input device')}
            </p>
          </div>
          <div className="settings-cell__control">
            <Select
              shape="rounded"
              value={selectedDevices.micDevice || 'default'}
              onValueChange={handleMicDeviceChange}
              disabled={disabled}
            >
              <SelectTrigger
                id="mic-selection"
                className="settings-cell__select settings-cell__device-select"
                placeholder={t('Select Microphone')}
              />
              <SelectContent>
                <SelectItem index={0} value="default">{t('Default Microphone')}</SelectItem>
                {inputDevices.map((device, index) => (
                  <SelectItem
                    key={device.name}
                    index={index + 1}
                    value={`${device.name} (${device.device_type.toLowerCase()})`}
                  >
                    {device.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
      </section>

      <section className="settings-section settings-cell">
        <div className="settings-cell__row">
          <span className="settings-cell__avatar" aria-hidden="true">
            <Speaker size={20} />
          </span>
          <div className="settings-cell__text">
            <Label htmlFor="system-selection" className="settings-cell__label">
              {t('System Audio')}
            </Label>
            <p className="settings-cell__caption">
              {outputDevices.length === 0
                ? t('No system audio devices found')
                : t('Computer and call audio')}
            </p>
          </div>

          <div className="settings-cell__control">
            <Select
              shape="rounded"
              value={selectedDevices.systemDevice || 'default'}
              onValueChange={handleSystemDeviceChange}
              disabled={disabled}
            >
              <SelectTrigger
                id="system-selection"
                className="settings-cell__select settings-cell__device-select"
                placeholder={t('Select System Audio')}
              />
              <SelectContent>
                <SelectItem index={0} value="default">{t('Default System Audio')}</SelectItem>
                {outputDevices.map((device, index) => (
                  <SelectItem
                    key={device.name}
                    index={index + 1}
                    value={`${device.name} (${device.device_type.toLowerCase()})`}
                  >
                    {device.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
      </section>
    </>
  );
}
