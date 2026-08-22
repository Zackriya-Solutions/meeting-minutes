import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import {
  isOpenSpecDependencyError,
  isOpenSpecNetworkError,
  isOpenSpecTimeoutError,
} from '@/lib/utils';
import { useI18n } from '@/hooks/useI18n';

type OpenSpecStatus = 'idle' | 'generating' | 'done' | 'error';

export type OpenSpecStateEvent =
  | 'start'
  | 'success'
  | 'failure'
  | 'reset_error'
  | 'reset';

export function advanceOpenSpecState(current: OpenSpecStatus, event: OpenSpecStateEvent): OpenSpecStatus {
  switch (event) {
    case 'start':
      return 'generating';
    case 'success':
      return 'done';
    case 'failure':
      return 'error';
    case 'reset_error':
      return current === 'error' ? 'idle' : current;
    case 'reset':
      return 'idle';
    default:
      return current;
  }
}

interface OpenSpecErrorPayload {
  code: string;
  message: string;
  stderr?: string | null;
}

type OpenSpecGenerationResult =
  | {
    type: 'success';
    zip_temp_path: string;
    suggested_filename: string;
    slug: string;
  }
  | {
    type: 'error';
    code: string;
    message: string;
    stderr?: string | null;
  };

interface SaveOpenSpecResult {
  cancelled: boolean;
  savedPath?: string;
  saved_path?: string;
}

interface SummaryProviderConfig {
  provider: string;
  model: string;
}

type OpenSpecInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

interface GenerateOpenSpecOptions {
  invokeFn?: OpenSpecInvoke;
  t: (key: string) => string;
  showToastError: (message: string, options?: {
    description?: string;
    duration?: number;
    action?: {
      label: string;
      onClick: () => void;
    };
  }) => void;
  showToastSuccess: (message: string) => void;
  showToastInfo: (message: string) => void;
}

export async function generateOpenSpecBundle(
  {
    meetingId,
    hasTranscript,
    resume = false,
  }: {
    meetingId: string;
    hasTranscript: boolean;
    resume?: boolean;
  },
  options: GenerateOpenSpecOptions,
): Promise<{ state: OpenSpecStatus; error: OpenSpecErrorPayload | null }> {
  if (!hasTranscript) {
    return { state: 'idle', error: null };
  }

  const invokeFn = options.invokeFn ?? invoke;

  // A meeting transcript can contain sensitive business information. Local
  // providers keep it on the device; every remote provider needs a fresh,
  // explicit decision for this meeting before we send the transcript/summary
  // to generate OpenSpec artifacts.
  try {
    const config = await invokeFn<SummaryProviderConfig | null>('api_get_model_config');
    const provider = config?.provider?.toLowerCase();
    const isRemoteProvider = provider && !['builtin-ai', 'ollama'].includes(provider);
    if (isRemoteProvider) {
      const approved = window.confirm(
        `OpenSpec will send this meeting's transcript and summary to ${config?.provider} (${config?.model}) to generate proposal, specification, design, and task artifacts. Continue?`,
      );
      if (!approved) {
        return { state: 'idle', error: null };
      }
    }
  } catch (error) {
    // Do not block local generation if an older backend cannot expose the
    // selected model config. The generation command still validates provider
    // configuration server-side.
    console.warn('[OpenSpec] Failed to check selected provider before generation:', error);
  }

  const showActionableError = async (payload: OpenSpecErrorPayload) => {
    const fallbackDescription = payload.stderr || payload.message;

    if (isOpenSpecDependencyError(payload.code, payload.message)) {
      options.showToastError(options.t('openspec.nodeRequired'), {
        description: fallbackDescription,
        duration: 9000,
        action: {
          label: options.t('openspec.installNode'),
          onClick: () => void invokeFn('open_external_url', { url: 'https://nodejs.org/en/download' }),
        },
      });
      return;
    }

    if (isOpenSpecNetworkError(payload.code, payload.message)) {
      options.showToastError(options.t('openspec.networkError'), {
        description: fallbackDescription,
      });
      return;
    }

    if (isOpenSpecTimeoutError(payload.code, payload.message)) {
      options.showToastError(options.t('openspec.timeout'), {
        description: fallbackDescription,
      });
      return;
    }

    options.showToastError(options.t('openspec.genericError'), {
      description: fallbackDescription,
    });
  };

  let result: OpenSpecGenerationResult;
  try {
    result = await invokeFn<OpenSpecGenerationResult>('api_generate_openspec_bundle', {
      meetingId,
      generateWithAi: true,
      resume: resume ?? false,
    });
  } catch (invokeError) {
    const payload = {
      code: 'cli_failed',
      message: invokeError instanceof Error ? invokeError.message : String(invokeError),
    };
    await showActionableError(payload);
    return { state: 'error', error: payload };
  }

  if (result.type === 'error') {
    const payload = {
      code: result.code,
      message: result.message,
      stderr: result.stderr,
    };
    await showActionableError(payload);
    return { state: 'error', error: payload };
  }

  const saveResult = await invokeFn<SaveOpenSpecResult>('api_save_openspec_bundle_as', {
    zipTempPath: result.zip_temp_path,
    suggestedFilename: result.suggested_filename,
  });

  if (saveResult.cancelled) {
    options.showToastInfo(options.t('openspec.done'));
  } else {
    options.showToastSuccess(options.t('openspec.done'));
  }

  return { state: 'done', error: null };
}

interface UseOpenSpecGenerationProps {
  meetingId: string;
  hasTranscript: boolean;
}

export function useOpenSpecGeneration({ meetingId, hasTranscript }: UseOpenSpecGenerationProps) {
  const { t } = useI18n();
  const [status, setStatus] = useState<OpenSpecStatus>('idle');
  const [error, setError] = useState<OpenSpecErrorPayload | null>(null);
  const [progress, setProgress] = useState<{ stage: string; message: string; percent: number } | null>(null);

  useEffect(() => {
    const unlisten = listen<{ meetingId: string; stage: string; message: string; percent: number }>(
      'openspec-generation-progress',
      (event) => {
        if (event.payload.meetingId === meetingId) {
          setProgress({ stage: event.payload.stage, message: event.payload.message, percent: event.payload.percent });
        }
      },
    );
    return () => { unlisten.then((fn) => fn()); };
  }, [meetingId]);

  const getStatusMessage = useCallback((value: OpenSpecStatus) => {
    switch (value) {
      case 'generating':
        return t('openspec.generating');
      case 'done':
        return t('openspec.done');
      case 'error':
        return t('openspec.error');
      default:
        return '';
    }
  }, [t]);

  const generate = useCallback(async (resume = false) => {
    if (!hasTranscript) {
      return;
    }

    setStatus(prev => advanceOpenSpecState(prev, 'start'));
    setError(null);
    setProgress({ stage: 'workspace', message: 'Preparing OpenSpec workspace', percent: 10 });

    const result = await generateOpenSpecBundle(
      { meetingId, hasTranscript, resume },
      {
        t,
        showToastError: (message, options) => toast.error(message, options),
        showToastSuccess: (message) => toast.success(message),
        showToastInfo: (message) => toast.info(message),
      },
    );

    if (result.state === 'error' && result.error) {
      setError(result.error);
      setStatus(prev => advanceOpenSpecState(prev, 'failure'));
      return;
    }

    setStatus(prev => advanceOpenSpecState(prev, 'success'));
  }, [hasTranscript, meetingId, t]);

  const cancel = useCallback(async () => {
    await invoke('cancel_openspec_generation', { meetingId });
    setProgress({ stage: 'cancelled', message: 'Cancelling generation', percent: 0 });
  }, [meetingId]);

  const handleGenerateOrRetry = useCallback(async () => {
    if (status === 'error') {
      setStatus(prev => advanceOpenSpecState(prev, 'reset_error'));
      setError(null);
      return;
    }

    await generate(true);
  }, [generate, status]);

  const handleRegenerate = useCallback(async () => {
    await generate();
  }, [generate]);

  return {
    openSpecStatus: status,
    openSpecError: error,
    getOpenSpecStatusMessage: getStatusMessage,
    handleGenerateOpenSpec: handleGenerateOrRetry,
    handleRegenerateOpenSpec: handleRegenerate,
    openSpecProgress: progress,
    cancelOpenSpecGeneration: cancel,
  };
}
