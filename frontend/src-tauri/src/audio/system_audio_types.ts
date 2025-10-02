// TypeScript type definitions for system audio functionality

export interface SystemAudioCommands {
  // Start system audio capture (returns success message)
  startSystemAudioCaptureCommand(): Promise<string>;

  // List available system audio devices
  listSystemAudioDevicesCommand(): Promise<string[]>;

  // Check if the app has permission to access system audio
  checkSystemAudioPermissionsCommand(): Promise<boolean>;

  // Start monitoring system audio usage by other applications
  startSystemAudioMonitoring(): Promise<void>;

  // Stop monitoring system audio usage
  stopSystemAudioMonitoring(): Promise<void>;

  // Get the current status of system audio monitoring
  getSystemAudioMonitoringStatus(): Promise<boolean>;
}

// Event types emitted by the system audio detector
export interface SystemAudioEvents {
  'system-audio-started': string[]; // Array of app names using system audio
  'system-audio-stopped': void;
}

// Example usage in React component:
/*
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// Start monitoring system audio
await invoke('start_system_audio_monitoring');

// Listen for system audio events
const unlisten = await listen<string[]>('system-audio-started', (event) => {
  console.log('Apps using system audio:', event.payload);
});

// Check permissions
const hasPermission = await invoke('check_system_audio_permissions_command');
if (!hasPermission) {
  console.warn('No system audio permissions');
}

// List available devices
const devices = await invoke('list_system_audio_devices_command');
console.log('System audio devices:', devices);

// Stop monitoring when component unmounts
await invoke('stop_system_audio_monitoring');
unlisten();
*/