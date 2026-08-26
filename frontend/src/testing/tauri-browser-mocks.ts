import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';

const meeting = {
  id: 'theme-test-meeting',
  title: 'Theme Test Meeting',
  created_at: '2026-06-20T09:00:00Z',
  updated_at: '2026-06-20T09:30:00Z',
  folder_path: '/tmp/theme-test-meeting',
  transcripts: [],
};

const completedOnboardingStatus = {
  version: '1.0',
  completed: true,
  current_step: 3,
  model_status: {
    parakeet: 'downloaded',
    summary: 'downloaded',
    selected_summary_model: 'qwen3.5:2b',
  },
  last_updated: '2026-06-20T09:30:00Z',
};

const newOnboardingStatus = {
  ...completedOnboardingStatus,
  completed: false,
  current_step: 1,
};

const browserStore = new Map<string, unknown>();

const knownNoopCommands = new Set([
  'set_audio_backend',
  'set_language_preference',
  'set_notification_settings',
  'set_recording_preferences',
  'start_analytics_session',
  'start_audio_level_monitoring',
  'start_import_audio_command',
  'start_recording',
  'start_recording_with_devices_and_meeting',
  'start_retranscription_command',
  'stop_audio_level_monitoring',
  'stop_recording',
  'track_analytics_disabled',
  'track_analytics_enabled',
  'track_analytics_transparency_viewed',
  'track_custom_prompt_used',
  'track_daily_active_user',
  'track_event',
  'track_feature_used',
  'track_meeting_deleted',
  'track_meeting_started',
  'track_model_changed',
  'track_recording_started',
  'track_recording_stopped',
  'track_settings_changed',
  'track_summary_generation_completed',
  'track_summary_regenerated',
  'track_user_first_launch',
]);

const commandFixtures: Record<string, unknown> = {
  // Startup update check (useUpdateCheck fires ~2s after mount): keep it
  // deterministic so long-lived E2E pages don't hit the fixture-missing throw.
  'plugin:app|version': '0.0.0-e2e',
  'plugin:updater|check': null,
  get_onboarding_status: completedOnboardingStatus,
  get_recording_state: { is_recording: false, is_paused: false },
  is_recording: false,
  api_get_meetings: [meeting],
  api_get_meeting: meeting,
  api_get_meeting_metadata: meeting,
  api_get_meeting_transcripts: {
    transcripts: [
      {
        id: 'theme-test-transcript',
        text: 'We approved the dark theme.',
        timestamp: '09:00:00',
        audio_start_time: 0,
        audio_end_time: 4.2,
        duration: 4.2,
        confidence: 0.98,
      },
    ],
    has_more: false,
    total_count: 1,
  },
  api_get_summary: {
    status: 'completed',
    meetingName: meeting.title,
    meeting_id: meeting.id,
    start: '2026-06-20T09:00:00Z',
    end: '2026-06-20T09:30:00Z',
    data: {
      markdown: '# Theme Test Meeting\n## Decisions\nUse semantic tokens.',
    },
    error: null,
  },
  api_get_meeting_summary_language: {
    language: null,
    storage: 'metadata',
  },
  api_get_model_config: {
    provider: 'builtin-ai',
    model: 'qwen3.5:2b',
    whisperModel: 'parakeet-tdt-0.6b-v3-int8',
    ollamaEndpoint: null,
  },
  api_get_transcript_config: {
    provider: 'parakeet',
    model: 'parakeet-tdt-0.6b-v3-int8',
    apiKey: null,
  },
  api_get_custom_openai_config: null,
  api_get_api_key: null,
  api_get_transcript_api_key: null,
  api_get_auto_generate_setting: false,
  api_list_templates: [],
  get_notification_settings: null,
  get_database_directory: '/tmp/Meetily',
  whisper_get_models_directory: '/tmp/Meetily/models',
  get_default_recordings_folder_path: '/tmp/Meetily Recordings',
  get_recording_preferences: {
    preferred_mic_device: null,
    preferred_system_device: null,
  },
  get_audio_devices: [],
  get_audio_backend_info: [
    {
      id: 'screencapturekit',
      name: 'ScreenCaptureKit',
      description: 'Native macOS system audio capture.',
    },
  ],
  get_current_audio_backend: 'screencapturekit',
  get_ollama_models: [],
  is_analytics_enabled: false,
  builtin_ai_list_models: [
    {
      name: 'qwen3.5:2b',
      display_name: 'Qwen 3.5 2B (Balanced)',
      status: { type: 'available' },
      path: '/tmp/models/Qwen3.5-2B-Q4_K_M.gguf',
      size_mb: 1221,
      context_size: 32768,
      description: 'Balanced local model for built-in summaries.',
      gguf_file: 'Qwen3.5-2B-Q4_K_M.gguf',
    },
    {
      name: 'qwen3.5:4b',
      display_name: 'Qwen 3.5 4B (High Quality)',
      status: { type: 'not_downloaded' },
      path: '/tmp/models/Qwen3.5-4B-Q4_K_M.gguf',
      size_mb: 2614,
      context_size: 32768,
      description: 'Higher-quality local model for built-in summaries.',
      gguf_file: 'Qwen3.5-4B-Q4_K_M.gguf',
    },
  ],
  builtin_ai_get_recommended_model: 'qwen3.5:2b',
  builtin_ai_download_model: null,
  check_first_launch: false,
  check_default_legacy_database: null,
  check_homebrew_database: null,
  initialize_fresh_database: null,
  import_and_initialize_database: null,
  parakeet_init: null,
  parakeet_has_available_models: true,
  parakeet_download_model: null,
  parakeet_retry_download: null,
  parakeet_get_available_models: [
    {
      name: 'parakeet-tdt-0.6b-v3-int8',
      path: '/tmp/models/parakeet-tdt-0.6b-v3-int8',
      size_mb: 670,
      speed: 'Ultra Fast (v3)',
      status: 'Available',
      description: 'Recommended local transcription model.',
      quantization: 'Int8',
    },
  ],
  save_onboarding_status_cmd: null,
  complete_onboarding: null,
  whisper_get_available_models: [],
};

let installed = false;

function recordNativeThemePayload(payload: unknown) {
  const target = window as typeof window & {
    __e2eTauriThemeCalls?: unknown[];
  };

  target.__e2eTauriThemeCalls = target.__e2eTauriThemeCalls ?? [];
  target.__e2eTauriThemeCalls.push(payload);
}

function getScenarioFixture(command: string) {
  const scenario = new URLSearchParams(window.location.search).get('__e2e');

  if (scenario === 'onboarding' && command === 'get_onboarding_status') {
    return newOnboardingStatus;
  }

  if (scenario === 'empty-home' && command === 'api_get_meetings') {
    return [];
  }

  return undefined;
}

export function installTauriBrowserMocks() {
  if (
    installed ||
    typeof window === 'undefined' ||
    process.env.NEXT_PUBLIC_E2E_TESTING !== '1'
  ) {
    return;
  }

  installed = true;
  mockWindows('main');
  mockIPC(
    (command, payload) => {
      const scenarioFixture = getScenarioFixture(command);
      if (scenarioFixture !== undefined) return scenarioFixture;
      if (command === 'plugin:store|load') return 1;
      if (
        command === 'plugin:store|has' ||
        command === 'plugin:store|get' ||
        command === 'plugin:store|set'
      ) {
        const key =
          typeof payload === 'object' &&
          payload !== null &&
          'key' in payload &&
          typeof payload.key === 'string'
            ? payload.key
            : '';

        if (command === 'plugin:store|has') return browserStore.has(key);
        if (command === 'plugin:store|get') {
          return [browserStore.get(key), browserStore.has(key)];
        }

        const value =
          typeof payload === 'object' &&
          payload !== null &&
          'value' in payload
            ? payload.value
            : undefined;
        browserStore.set(key, value);
        return null;
      }
      if (command === 'plugin:store|save') return null;
      if (command === 'plugin:app|set_app_theme') {
        recordNativeThemePayload(payload);
        return null;
      }
      if (command === 'builtin_ai_is_model_ready') {
        return (
          typeof payload === 'object' &&
          payload !== null &&
          'modelName' in payload &&
          payload.modelName === 'qwen3.5:2b'
        );
      }
      if (command in commandFixtures) return commandFixtures[command];

      if (knownNoopCommands.has(command)) return null;

      throw new Error(`[E2E fixture missing] ${command}`);
    },
    { shouldMockEvents: true },
  );
}
