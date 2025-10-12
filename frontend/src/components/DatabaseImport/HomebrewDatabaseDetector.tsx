'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Database, AlertCircle } from 'lucide-react';

interface HomebrewDatabaseDetectorProps {
  onDatabaseFound: (path: string) => void;
}

// Homebrew paths differ between Intel and Apple Silicon Macs
const HOMEBREW_PATHS = [
  '/opt/homebrew/var/meetily/meeting_minutes.db',  // Apple Silicon (M1/M2/M3)
  '/usr/local/var/meetily/meeting_minutes.db',      // Intel Macs
];

export function HomebrewDatabaseDetector({ onDatabaseFound }: HomebrewDatabaseDetectorProps) {
  const [isChecking, setIsChecking] = useState(true);
  const [homebrewDbExists, setHomebrewDbExists] = useState(false);
  const [dbSize, setDbSize] = useState<number>(0);
  const [detectedPath, setDetectedPath] = useState<string>('');

  useEffect(() => {
    checkHomebrewDatabase();
  }, []);

  const checkHomebrewDatabase = async () => {
    try {
      setIsChecking(true);

      // Check all possible Homebrew locations
      for (const path of HOMEBREW_PATHS) {
        const result = await invoke<{ exists: boolean; size: number } | null>('check_homebrew_database', {
          path,
        });

        if (result && result.exists && result.size > 0) {
          setHomebrewDbExists(true);
          setDbSize(result.size);
          setDetectedPath(path);
          // Auto-populate the detected path
          onDatabaseFound(path);
          break; // Stop checking once we find a valid database
        }
      }
    } catch (error) {
      console.error('Error checking homebrew database:', error);
      // Silently fail - this is just auto-detection
    } finally {
      setIsChecking(false);
    }
  };

  if (isChecking) {
    return null; // Don't show anything while checking
  }

  if (!homebrewDbExists) {
    return null; // Don't show if no database found
  }

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <div className="mb-4 p-4 bg-blue-50 border-2 border-blue-300 rounded-lg">
      <div className="flex items-start gap-3">
        <Database className="h-6 w-6 text-blue-600 mt-0.5 flex-shrink-0" />
        <div className="flex-1">
          <div className="flex items-center gap-2 mb-1">
            <AlertCircle className="h-4 w-4 text-blue-600" />
            <h3 className="text-sm font-semibold text-blue-900">
              Previous Meetily Installation Detected!
            </h3>
          </div>
          <p className="text-sm text-blue-800 mb-2">
            We found an existing database from your previous Meetily installation (Python backend version).
          </p>
          <div className="bg-white/50 rounded p-2 mb-2">
            <p className="text-xs text-blue-700 font-mono break-all">
              {detectedPath}
            </p>
            <p className="text-xs text-blue-600 mt-1">
              Size: {formatFileSize(dbSize)}
            </p>
          </div>
          <p className="text-xs text-blue-700">
            Click <span className="font-semibold">"Import Database"</span> below to migrate your meetings, transcripts, and summaries to the new Rust-powered backend.
          </p>
        </div>
      </div>
    </div>
  );
}

