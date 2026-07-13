'use client';

import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/components/ui/dialog';
import { Loader2, FolderOpen, Database, CheckCircle2, XCircle } from '@/components/memento/LucideCompat';
import { HomebrewDatabaseDetector } from './HomebrewDatabaseDetector';

interface LegacyDatabaseImportProps {
  isOpen: boolean;
  onComplete: () => void;
}

type ImportState = 'idle' | 'selecting' | 'detecting' | 'importing' | 'success' | 'error';

export function LegacyDatabaseImport({ isOpen, onComplete }: LegacyDatabaseImportProps) {
  const [importState, setImportState] = useState<ImportState>('idle');
  const [detectedPath, setDetectedPath] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string>('');

  const handleBrowse = async () => {
    try {
      setImportState('selecting');

      // Open file picker
      const selectedPath = await invoke<string | null>('select_legacy_database_path');

      if (!selectedPath) {
        setImportState('idle');
        return;
      }

      setImportState('detecting');

      // Detect database from selected path
      const dbPath = await invoke<string | null>('detect_legacy_database', {
        selectedPath,
      });

      if (dbPath) {
        setDetectedPath(dbPath);
        setImportState('idle');
      } else {
        setErrorMessage('No database found at selected location. Please select the Meetily folder, backend folder, or the database file directly.');
        setDetectedPath(null);
        setImportState('error');
        setTimeout(() => setImportState('idle'), 3000);
      }
    } catch (error) {
      console.error('Error browsing for database:', error);
      setErrorMessage(String(error));
      setImportState('error');
      setTimeout(() => setImportState('idle'), 3000);
    }
  };

  const handleImport = async () => {
    if (!detectedPath) return;

    try {
      setImportState('importing');

      await invoke('import_and_initialize_database', {
        legacyDbPath: detectedPath,
      });

      setImportState('success');
      toast.success('Database imported successfully! Reloading...');

      // Wait 1 second for user to see success, then reload window to refresh all data
      setTimeout(() => {
        window.location.reload();
      }, 1000);
    } catch (error) {
      console.error('Error importing database:', error);
      setErrorMessage(String(error));
      setImportState('error');
      toast.error(`Import failed: ${error}`);
      setTimeout(() => setImportState('idle'), 3000);
    }
  };

  const handleStartFresh = async () => {
    try {
      setImportState('importing');

      await invoke('initialize_fresh_database');

      setImportState('success');
      toast.success('Database initialized successfully! Starting app...');

      // Wait 1 second for user to see success, then reload window to start fresh
      setTimeout(() => {
        window.location.reload();
      }, 1000);
    } catch (error) {
      console.error('Error initializing database:', error);
      setErrorMessage(String(error));
      setImportState('error');
      toast.error(`Initialization failed: ${error}`);
      setTimeout(() => setImportState('idle'), 3000);
    }
  };

  const isLoading = ['selecting', 'detecting', 'importing'].includes(importState);
  const canImport = detectedPath && importState === 'idle';

  const handleHomebrewImportSuccess = () => {
    // The HomebrewDatabaseDetector handles the reload itself
    onComplete();
  };

  const handleHomebrewDecline = () => {
    // User declined homebrew import, they can continue with manual browse
  };

  return (
    <Dialog open={isOpen} onOpenChange={() => {}}>
      <DialogContent className="sm:max-w-[600px]" onPointerDownOutside={(e) => e.preventDefault()}>
        <DialogHeader>
          <DialogTitle className="text-2xl">Welcome to Meetily!</DialogTitle>
          <DialogDescription className="text-base pt-2">
            Do you have data from a previous Meetily installation?
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 py-4">
          {/* Homebrew Database Auto-Detection */}
          <HomebrewDatabaseDetector 
            onImportSuccess={handleHomebrewImportSuccess}
            onDecline={handleHomebrewDecline}
          />

          {/* Browse Section */}
          <div className="space-y-3">
            <p className="text-sm text-[var(--fg2)]">
              Select your previous Meetily folder, backend directory, or database file:
            </p>

            <button
              onClick={handleBrowse}
              disabled={isLoading}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-[var(--gold)] text-[var(--fg-inverse)] rounded-lg hover:bg-[var(--gold)] disabled:bg-[var(--fg3)] disabled:cursor-not-allowed transition-colors"
            >
              {importState === 'selecting' || importState === 'detecting' ? (
                <>
                  <Loader2 className="h-5 w-5 animate-spin" />
                  <span>{importState === 'selecting' ? 'Selecting...' : 'Detecting database...'}</span>
                </>
              ) : (
                <>
                  <FolderOpen className="h-5 w-5" />
                  <span>Browse for Database</span>
                </>
              )}
            </button>
          </div>

          {/* Detection Result */}
          {detectedPath && (
            <div className="p-3 bg-[color-mix(in_srgb,var(--success)_12%,transparent)] border border-[color-mix(in_srgb,var(--success)_42%,transparent)] rounded-lg">
              <div className="flex items-start gap-2">
                <CheckCircle2 className="h-5 w-5 text-[var(--success)] mt-0.5 flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-[var(--success)]">Database found!</p>
                  <p className="text-xs text-[var(--success)] mt-1 break-all">{detectedPath}</p>
                </div>
              </div>
            </div>
          )}

          {/* Error Message */}
          {importState === 'error' && errorMessage && (
            <div className="p-3 bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] border border-[color-mix(in_srgb,var(--danger)_42%,transparent)] rounded-lg">
              <div className="flex items-start gap-2">
                <XCircle className="h-5 w-5 text-[var(--danger)] mt-0.5 flex-shrink-0" />
                <div className="flex-1">
                  <p className="text-sm text-[var(--danger)]">{errorMessage}</p>
                </div>
              </div>
            </div>
          )}

          {/* Action Buttons */}
          <div className="flex flex-col gap-3 pt-2">
            <button
              onClick={handleImport}
              disabled={!canImport || isLoading}
              className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-[var(--success)] text-[var(--fg-inverse)] rounded-lg hover:bg-[var(--success)] disabled:bg-[var(--bg-elevated)] disabled:cursor-not-allowed transition-colors"
            >
              {importState === 'importing' ? (
                <>
                  <Loader2 className="h-5 w-5 animate-spin" />
                  <span>Importing...</span>
                </>
              ) : importState === 'success' ? (
                <>
                  <CheckCircle2 className="h-5 w-5" />
                  <span>Success!</span>
                </>
              ) : (
                <>
                  <Database className="h-5 w-5" />
                  <span>Import Database</span>
                </>
              )}
            </button>

            <div className="relative">
              <div className="absolute inset-0 flex items-center">
                <div className="w-full border-t border-[var(--border-strong)]"></div>
              </div>
              <div className="relative flex justify-center text-sm">
                <span className="px-2 bg-[var(--bg-canvas)] text-[var(--fg2)]">or</span>
              </div>
            </div>

            <button
              onClick={handleStartFresh}
              disabled={isLoading}
              className="w-full px-4 py-3 border-2 border-[var(--border-strong)] text-[var(--fg2)] rounded-lg hover:bg-[var(--bg-sheet)] disabled:bg-[var(--bg-elevated)] disabled:cursor-not-allowed transition-colors"
            >
              Start Fresh (No Import)
            </button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
