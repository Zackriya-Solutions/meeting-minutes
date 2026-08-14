import { useState, useEffect, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';

export type ModalType =
  | 'modelSettings'
  | 'deviceSettings'
  | 'languageSettings'
  | 'errorAlert'
  | 'chunkDropWarning';

interface ModalState {
  modelSettings: boolean;
  deviceSettings: boolean;
  languageSettings: boolean;
  errorAlert: boolean;
  chunkDropWarning: boolean;
}

interface ModalMessages {
  errorAlert: string;
  chunkDropWarning: string;
}

interface UseModalStateReturn {
  modals: ModalState;
  messages: ModalMessages;
  showModal: (name: ModalType, message?: string) => void;
  hideModal: (name: ModalType) => void;
  hideAllModals: () => void;
}

/**
 * Custom hook for managing all modal state and event listeners.
 * Consolidates modal state and recording-related event listeners from page.tsx.
 *
 * Features:
 * - Unified modal state management
 * - Event listeners for chunk drops and transcription errors
 */
export function useModalState(): UseModalStateReturn {
  // Modal visibility state
  const [modals, setModals] = useState<ModalState>({
    modelSettings: false,
    deviceSettings: false,
    languageSettings: false,
    errorAlert: false,
    chunkDropWarning: false,
  });

  // Modal messages
  const [messages, setMessages] = useState<ModalMessages>({
    errorAlert: '',
    chunkDropWarning: '',
  });

  // Show modal with optional message
  const showModal = useCallback((name: ModalType, message?: string) => {
    setModals(prev => ({ ...prev, [name]: true }));

    // Set message if provided
    if (message && (name === 'errorAlert' || name === 'chunkDropWarning')) {
      setMessages(prev => ({ ...prev, [name]: message }));
    }
  }, []);

  // Hide modal and clear its message
  const hideModal = useCallback((name: ModalType) => {
    setModals(prev => ({ ...prev, [name]: false }));

    // Clear message when closing
    if (name === 'errorAlert' || name === 'chunkDropWarning') {
      setMessages(prev => ({ ...prev, [name]: '' }));
    }
  }, []);

  // Hide all modals
  const hideAllModals = useCallback(() => {
    setModals({
      modelSettings: false,
      deviceSettings: false,
      languageSettings: false,
      errorAlert: false,
      chunkDropWarning: false,
    });
    setMessages({
      errorAlert: '',
      chunkDropWarning: '',
    });
  }, []);

  // Set up chunk drop warning listener
  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    const setupChunkDropListener = async () => {
      try {
        console.log('Setting up chunk-drop-warning listener...');
        unlistenFn = await listen<string>('chunk-drop-warning', (event) => {
          console.log('Chunk drop warning received:', event.payload);
          showModal('chunkDropWarning', event.payload);
        });
        console.log('Chunk drop warning listener setup complete');
      } catch (error) {
        console.error('Failed to setup chunk drop warning listener:', error);
      }
    };

    setupChunkDropListener();

    return () => {
      console.log('Cleaning up chunk drop warning listener...');
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, [showModal]);

  // Set up transcription error listener for model loading failures
  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    const setupTranscriptionErrorListener = async () => {
      try {
        console.log('Setting up transcription-error listener...');
        unlistenFn = await listen<{ error: string, userMessage: string, actionable: boolean }>('transcription-error', (event) => {
          console.log('Transcription error received:', event.payload);
          const { userMessage, actionable } = event.payload;

          // Keep model failures non-blocking: the user can open Settings → Transcription
          // when needed, without an unexpected modal taking over the current screen.
          toast.error('', {
            description: userMessage,
            duration: actionable ? 7000 : 5000,
          });
        });
        console.log('Transcription error listener setup complete');
      } catch (error) {
        console.error('Failed to setup transcription error listener:', error);
      }
    };

    setupTranscriptionErrorListener();

    return () => {
      console.log('Cleaning up transcription error listener...');
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, []);

  return {
    modals,
    messages,
    showModal,
    hideModal,
    hideAllModals,
  };
}
