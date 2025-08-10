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
      const store = await load('analytics.json', { autoSave: false });
      if (!(await store.has('analyticsOptedIn'))) {
        await store.set('analyticsOptedIn', true);
      }
      const analyticsOptedIn = await store.get('analyticsOptedIn')
      
      setIsAnalyticsOptedIn(analyticsOptedIn as boolean);
      if (analyticsOptedIn && isAnalyticsOptedIn) {
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
        
        // Identify user with enhanced properties immediately after init
        await Analytics.identify(userId, {
          app_version: '0.0.5',
          platform: 'tauri',
          first_seen: new Date().toISOString(),
          os: navigator.platform,
          user_agent: navigator.userAgent,
        });
        
        // Start analytics session with the same user ID
        await Analytics.startSession(userId);
        
        // Check and track first launch (after analytics is initialized)
        await Analytics.checkAndTrackFirstLaunch();
        
        // Track app started
        await Analytics.trackAppStarted();
        
        // Check and track daily usage
        await Analytics.checkAndTrackDailyUsage();
        
        // Set up cleanup on page unload
        const handleBeforeUnload = () => {
          Analytics.cleanup();
        };
        
        window.addEventListener('beforeunload', handleBeforeUnload);
        
        // Cleanup function
        return () => {
          window.removeEventListener('beforeunload', handleBeforeUnload);
          Analytics.cleanup();
        };
      
    };

    initAnalytics().catch(console.error);
  }, [isAnalyticsOptedIn]);

  return <AnalyticsContext.Provider value={{ isAnalyticsOptedIn, setIsAnalyticsOptedIn }}>{children}</AnalyticsContext.Provider>;
} 