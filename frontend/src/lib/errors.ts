import { toast } from 'sonner';

/** Text to show for a failed command: Tauri commands reject with the backend's message as a string. */
export function errorMessage(error: unknown, fallback: string): string {
  if (typeof error === 'string') return error;
  return error instanceof Error ? error.message : fallback;
}

/**
 * Runs a user action; a failure is logged and shown in a toast. Refusals from the backend (for
 * example while a speaker job runs) are written for the user. Returns whether it succeeded.
 */
export async function attempt(action: () => Promise<void>, failure: string): Promise<boolean> {
  try {
    await action();
    return true;
  } catch (error) {
    console.error(failure, error);
    toast.error(errorMessage(error, failure));
    return false;
  }
}
