'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Database, AlertCircle, Loader2, CheckCircle2 } from 'lucide-react';

interface HomebrewDatabaseDetectorProps {
  onImportSuccess: () => void;
  onDecline: () => void;
}

// Homebrew paths differ between Intel and Apple Silicon Macs
const HOMEBREW_PATHS = [
  '/opt/homebrew/var/meetily/meeting_minutes.db',  // Apple Silicon (M1/M2/M3)
  '/usr/local/var/meetily/meeting_minutes.db',      // Intel Macs
];

export function HomebrewDatabaseDetector({ onImportSuccess, onDecline }: HomebrewDatabaseDetectorProps) {
  const [isChecking, setIsChecking] = useState(true);
  const [isImporting, setIsImporting] = useState(false);
  const [homebrewDbExists, setHomebrewDbExists] = useState(false);
  const [dbSize, setDbSize] = useState<number>(0);
  const [detectedPath, setDetectedPath] = useState<string>('');
  const [isDismissed, setIsDismissed] = useState(false);

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

  const handleYes = async () => {
    try {
      setIsImporting(true);

      await invoke('import_and_initialize_database', {
        legacyDbPath: detectedPath,
      });

      toast.success('Database imported successfully! Reloading...');

      // Wait 1 second for user to see success, then reload window to refresh all data
      setTimeout(() => {
        window.location.reload();
      }, 1000);
    } catch (error) {
      console.error('Error importing database:', error);
      toast.error(`Import failed: ${error}`);
      setIsImporting(false);
    }
  };

  const handleNo = () => {
    setIsDismissed(true);
    onDecline();
  };

  if (isChecking || !homebrewDbExists || isDismissed) {
    return null;
  }

  const formatFileSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <div className="mb-4 p-4 bg-info/10 border-2 border-info/40 rounded-lg">
      <div className="flex items-start gap-3">
        <Database className="h-6 w-6 text-info-text mt-0.5 flex-shrink-0" />
        <div className="flex-1">
          <div className="flex items-center gap-2 mb-1">
            <AlertCircle className="h-4 w-4 text-info-text" />
            <h3 className="text-sm font-semibold text-info-text">
              Previous Meetily Installation Detected!
            </h3>
          </div>
          <p className="text-sm text-info-text mb-2">
            We found an existing database from your previous Meetily installation (Python backend version).
          </p>
          <div className="bg-card/50 rounded p-2 mb-3">
            <p className="text-xs text-info-text font-mono break-all">
              {detectedPath}
            </p>
            <p className="text-xs text-info-text mt-1">
              Size: {formatFileSize(dbSize)}
            </p>
          </div>
          <p className="text-sm text-info-text mb-3">
            Would you like to import your previous meetings, transcripts, and summaries?
          </p>
          
          {/* Yes/No Buttons */}
          <div className="flex gap-2">
            <button
              onClick={handleYes}
              disabled={isImporting}
              className="flex-1 flex items-center justify-center gap-2 px-4 py-2 bg-success text-success-foreground rounded-lg hover:bg-success/90 disabled:bg-muted-foreground/40 disabled:cursor-not-allowed transition-colors"
            >
              {isImporting ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  <span>Importing...</span>
                </>
              ) : (
                <>
                  <CheckCircle2 className="h-4 w-4" />
                  <span>Yes, Import</span>
                </>
              )}
            </button>
            
            <button
              onClick={handleNo}
              disabled={isImporting}
              className="flex-1 px-4 py-2 border-2 border-info/50 text-info-text rounded-lg hover:bg-info/15 disabled:bg-muted disabled:cursor-not-allowed transition-colors"
            >
              No, Browse Manually
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

