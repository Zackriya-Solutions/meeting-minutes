import React, { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Info, Loader2, Check, AlertCircle, ChevronDown } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { OnboardingContainer } from '../OnboardingContainer';
import { useOnboarding } from '@/contexts/OnboardingContext';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";

type ProbeState = 'idle' | 'checking' | 'reachable' | 'unreachable';

export function SetupOverviewStep() {
  const {
    goNext,
    transcriptionMode,
    setTranscriptionMode,
    remoteTranscriptionUrl,
    setRemoteTranscriptionUrl,
    summaryMode,
    setSummaryMode,
  } = useOnboarding();

  const [isMac, setIsMac] = useState(false);
  const [showBackendOptions, setShowBackendOptions] = useState(false);
  const [probe, setProbe] = useState<ProbeState>('idle');
  const [probeError, setProbeError] = useState<string | null>(null);

  useEffect(() => {
    const checkPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch (e) {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };
    checkPlatform();
  }, []);

  // A URL edit invalidates any previous probe result: never let a stale green
  // check authorise a URL the user has since changed.
  useEffect(() => {
    setProbe('idle');
    setProbeError(null);
  }, [remoteTranscriptionUrl]);

  const usesRemoteTranscription = transcriptionMode === 'remote';
  const trimmedUrl = remoteTranscriptionUrl.trim();

  const testRemoteServer = async () => {
    if (!trimmedUrl) return;
    setProbe('checking');
    setProbeError(null);
    try {
      const reachable = await invoke<boolean>('remote_whisper_check_health', {
        baseUrl: trimmedUrl,
      });
      setProbe(reachable ? 'reachable' : 'unreachable');
      if (!reachable) {
        setProbeError('The server did not respond to a health check.');
      }
    } catch (error) {
      setProbe('unreachable');
      setProbeError(error instanceof Error ? error.message : String(error));
    }
  };

  // Downloads are irreversible in practice (hundreds of MB), so a remote setup
  // must be proven reachable before we let the user leave this step.
  const canContinue = !usesRemoteTranscription || probe === 'reachable';

  const steps = [
    {
      number: 1,
      type: 'transcription',
      title: usesRemoteTranscription
        ? 'Use Remote Transcription Server'
        : 'Download Transcription Engine',
    },
    {
      number: 2,
      type: 'summarization',
      title: summaryMode === 'external'
        ? 'Configure Summarization in Settings'
        : 'Download Summarization Engine',
    },
  ];

  const handleContinue = () => {
    goNext();
  };

  const description = usesRemoteTranscription || summaryMode === 'external'
    ? 'Meetily will use the AI backends you configured below. Nothing extra is downloaded for them.'
    : 'Meetily requires that you download the Transcription & Summarization AI models for the software to work.';

  return (
    <OnboardingContainer
      title="Setup Overview"
      description={description}
      step={2}
      totalSteps={isMac ? 4 : 3}
    >
      <div className="flex flex-col items-center space-y-6">
        {/* Steps Card */}
        <div className="w-full max-w-md bg-white rounded-lg border border-gray-200 p-4">
          <div className="space-y-4">
            {steps.map((step) => {
              return (
                <div
                  key={step.number}
                  className={`flex items-start gap-4 p-1`}
                >
                  <div className="flex-1 ml-1">
                    <h3 className="font-medium text-gray-900 flex items-center gap-2">
                        Step {step.number} :  {step.title}

                        {step.type === "summarization" && (
                            <TooltipProvider>
                            <Tooltip>
                                <TooltipTrigger asChild>
                                <button className="text-gray-400 hover:text-gray-600">
                                    <Info className="w-4 h-4" />
                                </button>
                                </TooltipTrigger>
                                <TooltipContent className="max-w-xs text-sm">
                                You can also select external AI providers like OpenAI, Claude, or
                                Ollama for summary generation in settings.
                                </TooltipContent>
                            </Tooltip>
                            </TooltipProvider>
                        )}
                        </h3>
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Advanced: bring-your-own backends. Collapsed by default so the
            default download path stays exactly as it was. */}
        <div className="w-full max-w-md">
          <button
            type="button"
            onClick={() => setShowBackendOptions((v) => !v)}
            className="flex items-center gap-1.5 text-xs text-gray-600 hover:text-gray-900"
            aria-expanded={showBackendOptions}
          >
            <ChevronDown
              className={`w-3.5 h-3.5 transition-transform ${showBackendOptions ? '' : '-rotate-90'}`}
            />
            Already running your own AI servers?
          </button>

          {showBackendOptions && (
            <div className="mt-3 space-y-5 rounded-lg border border-gray-200 bg-gray-50 p-4">
              {/* Transcription backend */}
              <fieldset className="space-y-2">
                <legend className="text-sm font-medium text-gray-900">Transcription</legend>

                <label className="flex items-start gap-2 cursor-pointer">
                  <input
                    type="radio"
                    name="transcription-mode"
                    className="mt-1"
                    checked={!usesRemoteTranscription}
                    onChange={() => setTranscriptionMode('local')}
                  />
                  <span className="text-sm text-gray-700">
                    Download the local engine
                    <span className="text-gray-500"> (~670 MB, runs on this machine)</span>
                  </span>
                </label>

                <label className="flex items-start gap-2 cursor-pointer">
                  <input
                    type="radio"
                    name="transcription-mode"
                    className="mt-1"
                    checked={usesRemoteTranscription}
                    onChange={() => setTranscriptionMode('remote')}
                  />
                  <span className="text-sm text-gray-700">
                    Use a remote Whisper server
                    <span className="text-gray-500"> (nothing is downloaded)</span>
                  </span>
                </label>

                {usesRemoteTranscription && (
                  <div className="pl-6 pt-1 space-y-2">
                    <Label className="block text-xs font-medium text-gray-700">
                      Server URL
                    </Label>
                    <div className="flex gap-2">
                      <Input
                        type="text"
                        value={remoteTranscriptionUrl}
                        onChange={(e) => setRemoteTranscriptionUrl(e.target.value)}
                        placeholder="http://192.168.1.100:8093"
                        className="flex-1 focus:ring-1 focus:ring-blue-500 focus:border-blue-500"
                      />
                      <Button
                        type="button"
                        variant="outline"
                        onClick={testRemoteServer}
                        disabled={!trimmedUrl || probe === 'checking'}
                        className="shrink-0"
                      >
                        {probe === 'checking' ? (
                          <Loader2 className="w-4 h-4 animate-spin" />
                        ) : (
                          'Test'
                        )}
                      </Button>
                    </div>

                    {probe === 'reachable' && (
                      <p className="flex items-center gap-1.5 text-xs text-green-600">
                        <Check className="w-3.5 h-3.5" /> Server is reachable.
                      </p>
                    )}
                    {probe === 'unreachable' && (
                      <p className="flex items-start gap-1.5 text-xs text-red-600">
                        <AlertCircle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
                        <span>{probeError || 'Could not reach the server.'}</span>
                      </p>
                    )}

                    <p className="text-xs text-gray-500">
                      Base URL of an OpenAI-compatible{' '}
                      <code>/v1/audio/transcriptions</code> server, for example a
                      self-hosted faster-whisper instance. No API key required.
                    </p>
                  </div>
                )}
              </fieldset>

              {/* Summarization backend */}
              <fieldset className="space-y-2 border-t border-gray-200 pt-4">
                <legend className="text-sm font-medium text-gray-900">Summarization</legend>

                <label className="flex items-start gap-2 cursor-pointer">
                  <input
                    type="radio"
                    name="summary-mode"
                    className="mt-1"
                    checked={summaryMode === 'local'}
                    onChange={() => setSummaryMode('local')}
                  />
                  <span className="text-sm text-gray-700">
                    Download the built-in model
                    <span className="text-gray-500"> (runs on this machine)</span>
                  </span>
                </label>

                <label className="flex items-start gap-2 cursor-pointer">
                  <input
                    type="radio"
                    name="summary-mode"
                    className="mt-1"
                    checked={summaryMode === 'external'}
                    onChange={() => setSummaryMode('external')}
                  />
                  <span className="text-sm text-gray-700">
                    Set up a provider later in Settings
                    <span className="text-gray-500"> (OpenAI, Claude, Ollama, ... — nothing is downloaded)</span>
                  </span>
                </label>

                {summaryMode === 'external' && (
                  <p className="pl-6 text-xs text-gray-500">
                    Summaries stay unavailable until you pick a provider in
                    Settings → Summarization.
                  </p>
                )}
              </fieldset>
            </div>
          )}
        </div>

        {/* CTA Section */}
        <div className="w-full max-w-xs space-y-4">
          <Button
            onClick={handleContinue}
            disabled={!canContinue}
            className="w-full h-11 bg-gray-900 hover:bg-gray-800 text-white disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Let's Go
          </Button>
          {!canContinue && (
            <p className="text-center text-xs text-gray-500">
              Test the connection to your server before continuing.
            </p>
          )}
          <div className="text-center">
            <a
              href="https://github.com/Zackriya-Solutions/meeting-minutes"
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs text-gray-600 hover:underline"
            >
              Report issues on GitHub
            </a>
          </div>
        </div>
      </div>
    </OnboardingContainer>
  );
}
