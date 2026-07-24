import { useState, useEffect } from 'react';

export type Platform = 'macos' | 'windows' | 'linux' | 'android' | 'unknown';

// Extend Window type to include Tauri internals
declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

/**
 * Detect platform from user agent (fallback method)
 */
function detectPlatformFromUserAgent(): Platform {
  if (typeof navigator === 'undefined') return 'unknown';

  const userAgent = navigator.userAgent.toLowerCase();
  if (userAgent.includes('android')) {
    // Check before 'linux': Android user agents contain both
    return 'android';
  } else if (userAgent.includes('mac')) {
    return 'macos';
  } else if (userAgent.includes('win')) {
    return 'windows';
  } else if (userAgent.includes('linux')) {
    return 'linux';
  }
  return 'unknown';
}

/**
 * Hook to detect the current platform
 * Uses Tauri's OS plugin if available, falls back to user agent detection
 * @returns The current platform
 */
export function usePlatform(): Platform {
  const [currentPlatform, setCurrentPlatform] = useState<Platform>(() => detectPlatformFromUserAgent());

  useEffect(() => {
    async function detectPlatform() {
      // Check if Tauri is available
      if (typeof window === 'undefined' || !window.__TAURI_INTERNALS__) {
        // Not in Tauri environment, use user agent
        setCurrentPlatform(detectPlatformFromUserAgent());
        return;
      }

      try {
        // Dynamically import to avoid SSR issues
        const { platform } = await import('@tauri-apps/plugin-os');
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
            setCurrentPlatform('linux');
            break;
          case 'android':
            setCurrentPlatform('android');
            break;
          default:
            setCurrentPlatform('unknown');
        }
      } catch (error) {
        console.warn('[usePlatform] Tauri platform detection failed, using user agent:', error);
        setCurrentPlatform(detectPlatformFromUserAgent());
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
