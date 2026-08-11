import { useState, useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';

interface AutoNameResult {
  meeting_id: string;
  title: string;
  success: boolean;
}

interface UseAutoNamingProps {
  meetingId: string;
  onTitleUpdated?: (newTitle: string) => void;
}

/**
 * Hook for auto-generating meeting titles using LLM.
 *
 * Calls the Rust `api_auto_generate_title` command which sends the
 * transcript to the configured LLM and returns a concise title.
 * Also provides `shouldAutoName` to check if the current title
 * is a default timestamp.
 */
export function useAutoNaming({ meetingId, onTitleUpdated }: UseAutoNamingProps) {
  const [isAutoNaming, setIsAutoNaming] = useState(false);
  const [autoNameError, setAutoNameError] = useState<string | null>(null);

  /**
   * Check if the current title looks like a default timestamp.
   * Returns true if the title should be auto-renamed.
   */
  const shouldAutoName = useCallback(async (): Promise<boolean> => {
    try {
      const result = await invokeTauri<boolean>('api_should_auto_name', {
        meetingId,
      });
      return result;
    } catch (error) {
      console.error('[useAutoNaming] Error checking auto-name:', error);
      return false;
    }
  }, [meetingId]);

  /**
   * Trigger auto-naming for the current meeting.
   * Sends transcript to LLM, saves generated title to DB,
   * and notifies parent of the update.
   */
  const triggerAutoNaming = useCallback(async (): Promise<AutoNameResult | null> => {
    setIsAutoNaming(true);
    setAutoNameError(null);

    try {
      Analytics.trackFeatureUsed('auto_naming');

      const result = await invokeTauri<AutoNameResult>('api_auto_generate_title', {
        meetingId,
      });

      if (result.success) {
        toast.success(`Meeting renamed to: "${result.title}"`);
        onTitleUpdated?.(result.title);
      }

      return result;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error('[useAutoNaming] Error:', errorMessage);
      setAutoNameError(errorMessage);
      toast.error(`Auto-naming failed: ${errorMessage}`);
      return null;
    } finally {
      setIsAutoNaming(false);
    }
  }, [meetingId, onTitleUpdated]);

  /**
   * Auto-name if the title is a default timestamp.
   * Returns true if auto-naming was triggered, false if not needed.
   */
  const autoRenameIfNeeded = useCallback(async (): Promise<boolean> => {
    const needsRename = await shouldAutoName();
    if (needsRename) {
      await triggerAutoNaming();
      return true;
    }
    return false;
  }, [shouldAutoName, triggerAutoNaming]);

  return {
    isAutoNaming,
    autoNameError,
    shouldAutoName,
    triggerAutoNaming,
    autoRenameIfNeeded,
  };
}
