import React, { useContext, useState, useEffect } from 'react';
import { Switch } from '@/components/ui/switch';
import { Info } from 'lucide-react';
import { AnalyticsContext } from './AnalyticsProvider';
import { load } from '@tauri-apps/plugin-store';
import { invoke } from '@tauri-apps/api/core';
import { Analytics } from '@/lib/analytics';


export default function AnalyticsConsentSwitch() {
  const { setIsAnalyticsOptedIn, isAnalyticsOptedIn } = useContext(AnalyticsContext);
  
  // Load saved preference on component mount
  useEffect(() => {
    const loadPreference = async () => {
      try {
        const store = await load('analytics.json', { autoSave: false });
        const saved = await store.get<boolean>('analyticsOptedIn');
        if (saved !== null && saved !== undefined) {
          setIsAnalyticsOptedIn(saved);
        }
      } catch (error) {
        console.log('No saved analytics preference found, using default');
      }
    };
    loadPreference();
  }, [setIsAnalyticsOptedIn]);

  const handleToggle = async (enabled: boolean) => {
    try {
      const store = await load('analytics.json', { autoSave: false });
      await store.set('analyticsOptedIn', enabled);
      await store.save();
      
      if (enabled) {
        // Full analytics initialization (same as AnalyticsProvider)
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
        
        // Track app started (re-enabled)
        await Analytics.trackAppStarted();
        
        console.log('Analytics re-enabled successfully');
      } else {
        await Analytics.disable();
        console.log('Analytics disabled successfully');
      }
      
      setIsAnalyticsOptedIn(enabled);
    } catch (error) {
      console.error('Failed to toggle analytics:', error);
    }
  };

  const handlePrivacyPolicyClick = async () => {
    try {
      await invoke('open_external_url', { url: 'https://github.com/Zackriya-Solutions/meeting-minutes/blob/main/PRIVACY_POLICY.md' });
    } catch (error) {
      console.error('Failed to open privacy policy link:', error);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <h3 className="text-base font-semibold text-gray-800 mb-2">Usage Analytics</h3>
        <p className="text-sm text-gray-600 mb-4">
          Help us improve Meetily by sharing anonymous usage data. No personal content is collected—everything stays on your device.
        </p>
      </div>

      <div className="flex items-center justify-between p-3 bg-gray-50 rounded-lg border border-gray-200">
        <div>
          <h4 className="font-semibold text-gray-800">Enable Analytics</h4>
          <p className="text-sm text-gray-600">Anonymous usage patterns only</p>
        </div>
        <Switch
          checked={isAnalyticsOptedIn}
          onCheckedChange={handleToggle}
          className="ml-4"
        />
      </div>

      <div className="flex items-start gap-2 p-2 bg-blue-50 rounded border border-blue-200">
        <Info className="w-4 h-4 text-blue-600 mt-0.5 flex-shrink-0" />
        <div className="text-xs text-blue-700">
          <p className="mb-1">
            Your meetings, transcripts, and recordings remain completely private and local.
          </p>
          <button 
            onClick={handlePrivacyPolicyClick}
            className="text-blue-600 hover:text-blue-800 underline hover:no-underline"
          >
            View Privacy Policy
          </button>
        </div>
      </div>
    </div>
  );
}