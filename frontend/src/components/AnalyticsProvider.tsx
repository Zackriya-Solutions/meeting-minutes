'use client';

import React, { useEffect, ReactNode, useRef, useState, createContext } from 'react';
import Analytics from '@/lib/analytics';
import { load } from '@tauri-apps/plugin-store';


interface AnalyticsProviderProps {
  children: ReactNode;
}

interface AnalyticsContextType {
  isAnalyticsOptedIn: boolean;
  setIsAnalyticsOptedIn: (optedIn: boolean) => void;
}

export const AnalyticsContext = createContext<AnalyticsContextType>({
  isAnalyticsOptedIn: true,
  setIsAnalyticsOptedIn: () => {},
});

export default function AnalyticsProvider({ children }: AnalyticsProviderProps) {
  const [isAnalyticsOptedIn, setIsAnalyticsOptedIn] = useState(true);
  const initialized = useRef(false);

  useEffect(() => {
    // Prevent duplicate initialization in React StrictMode
    if (initialized.current) {
      return;
    }

    const initAnalytics = async () => {
      const store = await load('analytics.json', {
        autoSave: false,
        defaults: {
          analyticsOptedIn: true
        }
      });
      if (!(await store.has('analyticsOptedIn'))) {
        await store.set('analyticsOptedIn', true);
      }
      const analyticsOptedIn = await store.get('analyticsOptedIn')

      setIsAnalyticsOptedIn(analyticsOptedIn as boolean);
      // Fix: Use fresh value from store, not stale state
      if (analyticsOptedIn) {
        initAnalytics2();
      }
    }

    const initAnalytics2 = async () => {

        // Mark as initialized to prevent duplicates
        initialized.current = true;

        // Get persistent user ID FIRST (before initializing analytics)
        const userId = await Analytics.getPersistentUserId();

        // Initialize analytics
        await Analytics.init();

        // Get device info for initialization
        const deviceInfo = await Analytics.getDeviceInfo();

        // Store platform info in analytics.json for quick access
        const store = await load('analytics.json', {
          autoSave: false,
          defaults: {
            analyticsOptedIn: true
          }
        });
        await store.set('platform', deviceInfo.platform);
        await store.set('os_version', deviceInfo.os_version);
        await store.set('architecture', deviceInfo.architecture);

        // Set first launch date if not exists
        if (!(await store.has('first_launch_date'))) {
          await store.set('first_launch_date', new Date().toISOString());
        }

        await store.save();

        // Identify user with enhanced properties immediately after init
        await Analytics.identify(userId, {
          app_version: '0.1.1',
          platform: deviceInfo.platform,
          os_version: deviceInfo.os_version,
          architecture: deviceInfo.architecture,
          first_seen: new Date().toISOString(),
          user_agent: navigator.userAgent,
        });

        // Start analytics session with platform info
        const sessionId = await Analytics.startSession(userId);
        if (sessionId) {
          await Analytics.trackSessionStarted(sessionId);
        }

        // Check and track first launch (after analytics is initialized)
        await Analytics.checkAndTrackFirstLaunch();

        // Track app started
        await Analytics.trackAppStarted();

        // Check and track daily usage
        await Analytics.checkAndTrackDailyUsage();

        // Set up cleanup on page unload
        const handleBeforeUnload = async () => {
          if (sessionId) {
            await Analytics.trackSessionEnded(sessionId);
          }
          await Analytics.cleanup();
        };

        window.addEventListener('beforeunload', handleBeforeUnload);

        // Cleanup function
        return () => {
          window.removeEventListener('beforeunload', handleBeforeUnload);
          if (sessionId) {
            Analytics.trackSessionEnded(sessionId);
          }
          Analytics.cleanup();
        };

    };

    initAnalytics().catch(console.error);
  }, []); // Run only once on mount to prevent infinite loops

  // Separate effect to handle re-initialization when analytics is toggled
  useEffect(() => {
    // Reset initialized flag when analytics is disabled to allow re-initialization
    if (!isAnalyticsOptedIn) {
      initialized.current = false;
    }
  }, [isAnalyticsOptedIn]);

  return <AnalyticsContext.Provider value={{ isAnalyticsOptedIn, setIsAnalyticsOptedIn }}>{children}</AnalyticsContext.Provider>;
} 