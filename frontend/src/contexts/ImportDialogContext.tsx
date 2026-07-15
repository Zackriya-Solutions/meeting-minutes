'use client';

import { createContext, useContext, useCallback, ReactNode } from 'react';
import { useConfig } from './ConfigContext';
import { toast } from 'sonner';
import i18n from '@/i18n/config';

interface ImportDialogContextType {
  openImportDialog: (filePath?: string | null) => void;
}

const ImportDialogContext = createContext<ImportDialogContextType | null>(null);

export const useImportDialog = () => {
  const ctx = useContext(ImportDialogContext);
  if (!ctx) throw new Error('useImportDialog must be used within ImportDialogProvider');
  return ctx;
};

interface ImportDialogProviderProps {
  children: ReactNode;
  onOpen: (filePath?: string | null) => void;
}

export function ImportDialogProvider({ children, onOpen }: ImportDialogProviderProps) {
  const { betaFeatures } = useConfig();

  const openImportDialog = useCallback((filePath?: string | null) => {
    // Gate: Check beta feature flag before opening dialog
    if (!betaFeatures.importAndRetranscribe) {
      toast.error(i18n.t('notifications.betaFeatureDisabled'), {
        description: i18n.t('notifications.betaFeatureDisabledDesc')
      });
      return;
    }

    onOpen(filePath);
  }, [onOpen, betaFeatures]);

  return (
    <ImportDialogContext.Provider value={{ openImportDialog }}>
      {children}
    </ImportDialogContext.Provider>
  );
}
