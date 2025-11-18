import { useState, useEffect } from 'react';
import { platform } from '@tauri-apps/plugin-os';

export type Platform = 'macos' | 'windows' | 'linux' | 'unknown';

/**
 * Hook to detect the current platform using Tauri's OS plugin
 * @returns The current platform
 */
export function usePlatform(): Platform {
  const [currentPlatform, setCurrentPlatform] = useState<Platform>('unknown');

  useEffect(() => {
    async function detectPlatform() {
      try {
        const platformName = await platform();

        // Map Tauri's platform names to our simplified types
        switch (platformName) {
          case 'macos':
          case 'ios':
            setCurrentPlatform('macos');
            break;
          case 'windows':
            setCurrentPlatform('windows');
            break;
          case 'linux':
          case 'android':
            setCurrentPlatform('linux');
            break;
          default:
            setCurrentPlatform('unknown');
        }
      } catch (error) {
        console.error('Failed to detect platform:', error);
        // Fallback to navigator.userAgent if Tauri call fails
        const userAgent = navigator.userAgent.toLowerCase();
        if (userAgent.includes('mac')) {
          setCurrentPlatform('macos');
        } else if (userAgent.includes('win')) {
          setCurrentPlatform('windows');
        } else if (userAgent.includes('linux')) {
          setCurrentPlatform('linux');
        } else {
          setCurrentPlatform('unknown');
        }
      }
    }

    detectPlatform();
  }, []);

  return currentPlatform;
}

/**
 * Simple helper to check if the current platform is Linux
 * @returns true if running on Linux
 */
export function useIsLinux(): boolean {
  const currentPlatform = usePlatform();
  return currentPlatform === 'linux';
}
