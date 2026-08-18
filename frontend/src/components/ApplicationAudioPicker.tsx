import { useEffect, useState } from 'react';
import { RefreshCw, Volume2 } from 'lucide-react';
import {
  ApplicationAudioStream,
  AudioCaptureSelection,
  recordingService,
} from '@/services/recordingService';

interface ApplicationAudioPickerProps {
  onStart: (selection: AudioCaptureSelection) => void | Promise<void>;
  onCancel: () => void;
  disabled?: boolean;
}

export function ApplicationAudioPicker({
  onStart,
  onCancel,
  disabled = false,
}: ApplicationAudioPickerProps) {
  const [streams, setStreams] = useState<ApplicationAudioStream[]>([]);
  const [selectedSerial, setSelectedSerial] = useState<number | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadStreams = async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await recordingService.listApplicationAudioStreams();
      setStreams(result);
      setSelectedSerial((current) =>
        current !== null && result.some((stream) => stream.object_serial === current)
          ? current
          : null
      );
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void loadStreams();
  }, []);

  const selectedStream = streams.find((stream) => stream.object_serial === selectedSerial);

  return (
    <div className="mb-3 w-full max-w-xl rounded-xl border border-gray-200 bg-white p-4 shadow-xl">
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <h3 className="font-semibold text-gray-900">Choose audio source for this meeting</h3>
          <p className="mt-1 text-xs text-gray-600">
            Select one PipeWire application/media stream. This is not exact window or browser-tab isolation.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void loadStreams()}
          disabled={isLoading || disabled}
          className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 disabled:opacity-50"
          title="Refresh application streams"
        >
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </button>
      </div>

      {error && (
        <div className="mb-3 rounded-md border border-red-200 bg-red-50 p-3 text-xs text-red-700">
          {error}
        </div>
      )}

      <div className="max-h-48 space-y-2 overflow-y-auto">
        {isLoading && <p className="py-3 text-sm text-gray-500">Looking for application/media streams…</p>}
        {!isLoading && streams.length === 0 && (
          <p className="rounded-md bg-gray-50 p-3 text-sm text-gray-600">
            No separable application/media output streams are available. Use global system audio instead.
          </p>
        )}
        {!isLoading && streams.map((stream) => (
          <button
            type="button"
            key={stream.object_serial}
            onClick={() => setSelectedSerial(stream.object_serial)}
            disabled={disabled}
            className={`flex w-full items-start gap-3 rounded-lg border p-3 text-left transition-colors ${
              selectedSerial === stream.object_serial
                ? 'border-blue-500 bg-blue-50'
                : 'border-gray-200 hover:border-gray-400 hover:bg-gray-50'
            }`}
          >
            <Volume2 className="mt-0.5 h-4 w-4 shrink-0 text-gray-500" />
            <span className="min-w-0 text-sm">
              <span className="block truncate font-medium text-gray-900">
                {stream.application_name} · {stream.media_name}
              </span>
              <span className="block truncate text-xs text-gray-600">
                Process: {stream.process_name} · Stream: {stream.node_name}
              </span>
            </span>
          </button>
        ))}
      </div>

      <div className="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-gray-100 pt-3">
        <button
          type="button"
          onClick={() => void onStart({ mode: 'global' })}
          disabled={disabled}
          className="rounded-md border border-gray-300 px-3 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 disabled:opacity-50"
        >
          Use global system audio
        </button>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={disabled}
            className="rounded-md px-3 py-2 text-sm text-gray-600 hover:bg-gray-100 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => selectedStream && void onStart({
              mode: 'application',
              object_serial: selectedStream.object_serial,
              application_name: selectedStream.application_name,
              media_name: selectedStream.media_name,
              process_name: selectedStream.process_name,
            })}
            disabled={disabled || selectedStream === undefined}
            className="rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            Use selected stream
          </button>
        </div>
      </div>
    </div>
  );
}
