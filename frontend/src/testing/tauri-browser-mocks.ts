import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';

const meeting = {
  id: 'theme-test-meeting',
  title: 'Theme Test Meeting',
  created_at: '2026-06-20T09:00:00Z',
  updated_at: '2026-06-20T09:30:00Z',
  folder_path: '/tmp/theme-test-meeting',
  transcripts: [],
};

const commandFixtures: Record<string, unknown> = {
  get_onboarding_status: { completed: true },
  get_recording_state: { is_recording: false, is_paused: false },
  is_recording: false,
  api_get_meetings: [meeting],
  api_get_meeting: meeting,
  api_get_meeting_metadata: meeting,
  api_get_meeting_transcripts: [
    {
      id: 1,
      text: 'We approved the dark theme.',
      timestamp: 0,
      confidence: 0.98,
    },
  ],
  api_get_summary: {
    status: 'completed',
    data: '# Theme Test Meeting\n## Decisions\nUse semantic tokens.',
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
  get_default_recordings_folder_path: '/tmp/Meetily Recordings',
  get_recording_preferences: {
    preferred_mic_device: null,
    preferred_system_device: null,
  },
  get_audio_devices: [],
  get_ollama_models: [],
  is_analytics_enabled: false,
  builtin_ai_list_models: [],
  parakeet_get_available_models: [],
  whisper_get_available_models: [],
};

let installed = false;

function getScenarioFixture(command: string) {
  const scenario = new URLSearchParams(window.location.search).get('__e2e');

  if (scenario === 'onboarding' && command === 'get_onboarding_status') {
    return { completed: false };
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
    (command) => {
      const scenarioFixture = getScenarioFixture(command);
      if (scenarioFixture !== undefined) return scenarioFixture;
      if (command in commandFixtures) return commandFixtures[command];

      if (
        command.startsWith('track_') ||
        command.startsWith('plugin:') ||
        command.startsWith('set_') ||
        command.startsWith('start_') ||
        command.startsWith('stop_')
      ) {
        return null;
      }

      throw new Error(`[E2E fixture missing] ${command}`);
    },
    { shouldMockEvents: true },
  );
}
