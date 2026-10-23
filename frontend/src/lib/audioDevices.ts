import { invoke } from '@tauri-apps/api/core';

export interface AudioDevice {
  /** Exact device ID. Identity only: never shown to the user when a label exists. */
  name: string;
  device_type: 'Input' | 'Output';
  /** Human-readable label. Absent on macOS/Windows. */
  label?: string;
  is_default?: boolean;
}

export interface SelectedDevices {
  micDevice: string | null;
  systemDevice: string | null;
}

const DEFAULT_OPTION_VALUE = 'default:';
const DEVICE_OPTION_PREFIX = 'device:';

export function encodeDeviceOption(id: string | null): string {
  return id === null ? DEFAULT_OPTION_VALUE : `${DEVICE_OPTION_PREFIX}${id}`;
}

/**
 * Returns the device ID, null for the Default option, or undefined when the
 * value is not a recognised option value.
 */
export function decodeDeviceOption(value: string): string | null | undefined {
  if (value === DEFAULT_OPTION_VALUE) return null;
  if (value.startsWith(DEVICE_OPTION_PREFIX)) {
    const id = value.slice(DEVICE_OPTION_PREFIX.length);
    return id.length > 0 ? id : undefined;
  }
  return undefined;
}

export function deviceLabel(device: Pick<AudioDevice, 'name' | 'label'>): string {
  const label = device.label?.trim();
  return label ? label : device.name;
}

export function labelForId(devices: readonly AudioDevice[], id: string | null): string | null {
  if (id === null) return null;
  const match = devices.find((d) => d.name === id);
  return match ? deviceLabel(match) : id;
}

export function isDeviceAvailable(
  devices: readonly AudioDevice[],
  type: AudioDevice['device_type'],
  id: string,
): boolean {
  return devices.some((d) => d.device_type === type && d.name === id);
}

const labelCache = new Map<string, string>();

function rememberLabels(devices: readonly AudioDevice[]): void {
  for (const device of devices) {
    labelCache.set(device.name, deviceLabel(device));
  }
}

export async function fetchAudioDevices(): Promise<AudioDevice[]> {
  const devices = await invoke<AudioDevice[]>('get_audio_devices');
  rememberLabels(devices);
  return devices;
}

export function cachedDeviceLabel(id: string): string {
  return labelCache.get(id) ?? id;
}

export async function resolveDeviceLabel(id: string): Promise<string> {
  const cached = labelCache.get(id);
  if (cached !== undefined) return cached;
  try {
    await fetchAudioDevices();
  } catch (err) {
    console.error('Failed to resolve device label:', err);
  }
  return cachedDeviceLabel(id);
}
