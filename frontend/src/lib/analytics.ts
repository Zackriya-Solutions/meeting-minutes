import { invoke } from '@tauri-apps/api/core';

export interface AnalyticsProperties {
  [key: string]: string;
}

export interface UserSession {
  session_id: string;
  user_id: string;
  start_time: string;
  last_heartbeat: string;
  is_active: boolean;
}

export class Analytics {
  private static initialized = false;
  private static currentUserId: string | null = null;
  private static initializationPromise: Promise<void> | null = null;

  static async init(): Promise<void> {
    // Prevent duplicate initialization
    if (this.initialized) {
      return;
    }

    // If already initializing, wait for it to complete
    if (this.initializationPromise) {
      return this.initializationPromise;
    }

    this.initializationPromise = this.doInit();
    return this.initializationPromise;
  }

  private static async doInit(): Promise<void> {
    try {
      await invoke('init_analytics');
      this.initialized = true;
      console.log('Analytics initialized successfully');
    } catch (error) {
      console.error('Failed to initialize analytics:', error);
      throw error;
    } finally {
      this.initializationPromise = null;
    }
  }

  static async disable(): Promise<void> {
    try {
      await invoke('disable_analytics');
      this.initialized = false;
      this.currentUserId = null;
      this.initializationPromise = null;
      console.log('Analytics disabled successfully');
    } catch (error) {
      console.error('Failed to disable analytics:', error);
    }
  }

  static async isEnabled(): Promise<boolean> {
    try {
      return await invoke('is_analytics_enabled');
    } catch (error) {
      console.error('Failed to check analytics status:', error);
      return false;
    }
  }

  static async track(eventName: string, properties?: AnalyticsProperties): Promise<void> {
    if (!this.initialized) {
      console.warn('Analytics not initialized');
      return;
    }

    try {
      await invoke('track_event', { eventName, properties });
    } catch (error) {
      console.error(`Failed to track event ${eventName}:`, error);
    }
  }

  static async identify(userId: string, properties?: AnalyticsProperties): Promise<void> {
    if (!this.initialized) {
      console.warn('Analytics not initialized');
      return;
    }

    try {
      await invoke('identify_user', { userId, properties });
      this.currentUserId = userId;
    } catch (error) {
      console.error(`Failed to identify user ${userId}:`, error);
    }
  }

  // Enhanced user tracking methods for Phase 1
  static async startSession(userId: string): Promise<string | null> {
    if (!this.initialized) {
      console.warn('Analytics not initialized');
      return null;
    }

    try {
      const sessionId = await invoke('start_analytics_session', { userId });
      this.currentUserId = userId;
      
      return sessionId as string;
    } catch (error) {
      console.error('Failed to start analytics session:', error);
      return null;
    }
  }

  static async endSession(): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('end_analytics_session');
    } catch (error) {
      console.error('Failed to end analytics session:', error);
    }
  }

  static async trackDailyActiveUser(): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('track_daily_active_user');
    } catch (error) {
      console.error('Failed to track daily active user:', error);
    }
  }

  static async trackUserFirstLaunch(): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('track_user_first_launch');
    } catch (error) {
      console.error('Failed to track user first launch:', error);
    }
  }

  static async isSessionActive(): Promise<boolean> {
    if (!this.initialized) return false;

    try {
      return await invoke('is_analytics_session_active');
    } catch (error) {
      console.error('Failed to check session status:', error);
      return false;
    }
  }

  // User ID management with persistent storage
  static async getPersistentUserId(): Promise<string> {
    try {
      // First check if we have a stored user ID
      const { Store } = await import('@tauri-apps/plugin-store');
      const store = await Store.load('analytics.json');
      
      let userId = await store.get<string>('user_id');
      
      if (!userId) {
        // Generate new user ID
        userId = `user_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
        await store.set('user_id', userId);
        await store.set('is_first_launch', true);
        await store.save();
      }
      
      return userId;
    } catch (error) {
      console.error('Failed to get persistent user ID:', error);
      // Fallback to session storage
      let userId = sessionStorage.getItem('meetily_user_id');
      if (!userId) {
        userId = `user_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
        sessionStorage.setItem('meetily_user_id', userId);
        sessionStorage.setItem('is_first_launch', 'true');
      }
      return userId;
    }
  }

  static async checkAndTrackFirstLaunch(): Promise<void> {
    try {
      const { Store } = await import('@tauri-apps/plugin-store');
      const store = await Store.load('analytics.json');
      
      const isFirstLaunch = await store.get<boolean>('is_first_launch');
      
      if (isFirstLaunch) {
        await this.trackUserFirstLaunch();
        await store.set('is_first_launch', false);
        await store.save();
      }
    } catch (error) {
      console.error('Failed to check first launch:', error);
      // Fallback to session storage
      const isFirstLaunch = sessionStorage.getItem('is_first_launch') === 'true';
      if (isFirstLaunch) {
        await this.trackUserFirstLaunch();
        sessionStorage.removeItem('is_first_launch');
      }
    }
  }

  static async checkAndTrackDailyUsage(): Promise<void> {
    try {
      const { Store } = await import('@tauri-apps/plugin-store');
      const store = await Store.load('analytics.json');
      
      const today = new Date().toISOString().split('T')[0];
      const lastTrackedDate = await store.get<string>('last_daily_tracked');
      
      if (lastTrackedDate !== today) {
        await this.trackDailyActiveUser();
        await store.set('last_daily_tracked', today);
        await store.save();
      }
    } catch (error) {
      console.error('Failed to check daily usage:', error);
    }
  }

  static getCurrentUserId(): string | null {
    return this.currentUserId;
  }

  // Meeting-specific tracking methods
  static async trackMeetingStarted(meetingId: string, meetingTitle: string): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('track_meeting_started', { meetingId, meetingTitle });
    } catch (error) {
      console.error('Failed to track meeting started:', error);
    }
  }

  static async trackRecordingStarted(meetingId: string): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('track_recording_started', { meetingId });
    } catch (error) {
      console.error('Failed to track recording started:', error);
    }
  }

  static async trackRecordingStopped(meetingId: string, durationSeconds?: number): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('track_recording_stopped', { meetingId, durationSeconds });
    } catch (error) {
      console.error('Failed to track recording stopped:', error);
    }
  }

  static async trackMeetingDeleted(meetingId: string): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('track_meeting_deleted', { meetingId });
    } catch (error) {
      console.error('Failed to track meeting deleted:', error);
    }
  }

  static async trackSearchPerformed(query: string, resultsCount: number): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('track_search_performed', { query, resultsCount });
    } catch (error) {
      console.error('Failed to track search performed:', error);
    }
  }

  static async trackSettingsChanged(settingType: string, newValue: string): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('track_settings_changed', { settingType, newValue });
    } catch (error) {
      console.error('Failed to track settings changed:', error);
    }
  }

  static async trackFeatureUsed(featureName: string): Promise<void> {
    if (!this.initialized) return;

    try {
      await invoke('track_feature_used', { featureName });
    } catch (error) {
      console.error('Failed to track feature used:', error);
    }
  }

  // Convenience methods for common events
  static async trackPageView(pageName: string): Promise<void> {
    await this.track(`page_view_${pageName}`, { page: pageName });
  }

  static async trackButtonClick(buttonName: string, location?: string): Promise<void> {
    const properties: AnalyticsProperties = { button: buttonName };
    if (location) properties.location = location;
    await this.track(`button_click_${buttonName}`, properties);
  }

  static async trackError(errorType: string, errorMessage: string): Promise<void> {
    await this.track('error', { 
      error_type: errorType, 
      error_message: errorMessage 
    });
  }

  static async trackAppStarted(): Promise<void> {
    await this.track('app_started', { 
      timestamp: new Date().toISOString() 
    });
  }

  // Cleanup method for app shutdown
  static async cleanup(): Promise<void> {
    await this.endSession();
  }

  // Reset initialization state (useful for testing)
  static reset(): void {
    this.initialized = false;
    this.currentUserId = null;
    this.initializationPromise = null;
  }

  // Wait for analytics to be initialized
  static async waitForInitialization(timeout: number = 5000): Promise<boolean> {
    if (this.initialized) {
      return true;
    }
    
    const startTime = Date.now();
    while (!this.initialized && (Date.now() - startTime) < timeout) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    
    return this.initialized;
  }

  // Track backend connection success/failure
  static async trackBackendConnection(success: boolean, error?: string) {
    // Wait for analytics to be initialized
    const isInitialized = await this.waitForInitialization();
    if (!isInitialized) {
      console.warn('Analytics not initialized within timeout, skipping backend connection tracking');
      return;
    }

    try {
      console.log('Tracking backend connection event:', { success, error });
      await invoke('track_event', {
        eventName: 'backend_connection',
        properties: {
          success: success.toString(),
          error: error || '',
          timestamp: new Date().toISOString()
        }
      });
      console.log('Backend connection event tracked successfully');
    } catch (error) {
      console.error('Failed to track backend connection:', error);
    }
  }

  // Track transcription errors
  static async trackTranscriptionError(errorMessage: string) {
    if (!this.initialized) {
      console.warn('Analytics not initialized, skipping transcription error tracking');
      return;
    }

    try {
      console.log('Tracking transcription error event:', { errorMessage });
      await invoke('track_event', {
        eventName: 'transcription_error',
        properties: {
          error_message: errorMessage,
          timestamp: new Date().toISOString()
        }
      });
      console.log('Transcription error event tracked successfully');
    } catch (error) {
      console.error('Failed to track transcription error:', error);
    }
  }

  // Track transcription success
  static async trackTranscriptionSuccess(duration?: number) {
    if (!this.initialized) {
      console.warn('Analytics not initialized, skipping transcription success tracking');
      return;
    }

    try {
      console.log('Tracking transcription success event:', { duration });
      await invoke('track_event', {
        eventName: 'transcription_success',
        properties: {
          duration: duration ? duration.toString() : null,
          timestamp: new Date().toISOString()
        }
      });
      console.log('Transcription success event tracked successfully');
    } catch (error) {
      console.error('Failed to track transcription success:', error);
    }
  }

  // Summary generation analytics
  static async trackSummaryGenerationStarted(modelProvider: string, modelName: string, transcriptLength: number) {
    if (!this.initialized) {
      console.warn('Analytics not initialized, skipping summary generation started tracking');
      return;
    }

    try {
      console.log('Tracking summary generation started event:', { modelProvider, modelName, transcriptLength });
      await invoke('track_summary_generation_started', {
        modelProvider,
        modelName,
        transcriptLength
      });
      console.log('Summary generation started event tracked successfully');
    } catch (error) {
      console.error('Failed to track summary generation started:', error);
    }
  }

  static async trackSummaryGenerationCompleted(
    modelProvider: string, 
    modelName: string, 
    success: boolean, 
    durationSeconds?: number, 
    errorMessage?: string
  ) {
    if (!this.initialized) {
      console.warn('Analytics not initialized, skipping summary generation completed tracking');
      return;
    }

    try {
      console.log('Tracking summary generation completed event:', { modelProvider, modelName, success, durationSeconds, errorMessage });
      await invoke('track_summary_generation_completed', {
        modelProvider,
        modelName,
        success,
        durationSeconds,
        errorMessage
      });
      console.log('Summary generation completed event tracked successfully');
    } catch (error) {
      console.error('Failed to track summary generation completed:', error);
    }
  }

  static async trackSummaryRegenerated(modelProvider: string, modelName: string) {
    if (!this.initialized) {
      console.warn('Analytics not initialized, skipping summary regenerated tracking');
      return;
    }

    try {
      console.log('Tracking summary regenerated event:', { modelProvider, modelName });
      await invoke('track_summary_regenerated', {
        modelProvider,
        modelName
      });
      console.log('Summary regenerated event tracked successfully');
    } catch (error) {
      console.error('Failed to track summary regenerated:', error);
    }
  }

  static async trackModelChanged(oldProvider: string, oldModel: string, newProvider: string, newModel: string) {
    if (!this.initialized) {
      console.warn('Analytics not initialized, skipping model changed tracking');
      return;
    }

    try {
      console.log('Tracking model changed event:', { oldProvider, oldModel, newProvider, newModel });
      await invoke('track_model_changed', {
        oldProvider,
        oldModel,
        newProvider,
        newModel
      });
      console.log('Model changed event tracked successfully');
    } catch (error) {
      console.error('Failed to track model changed:', error);
    }
  }

  static async trackCustomPromptUsed(promptLength: number) {
    if (!this.initialized) {
      console.warn('Analytics not initialized, skipping custom prompt used tracking');
      return;
    }

    try {
      console.log('Tracking custom prompt used event:', { promptLength });
      await invoke('track_custom_prompt_used', {
        promptLength
      });
      console.log('Custom prompt used event tracked successfully');
    } catch (error) {
      console.error('Failed to track custom prompt used:', error);
    }
  }
}

export default Analytics; 