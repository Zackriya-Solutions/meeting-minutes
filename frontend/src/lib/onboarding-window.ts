import { invoke, isTauri } from '@tauri-apps/api/core';
import { LogicalSize, getCurrentWindow } from '@tauri-apps/api/window';
import {
  ONBOARDING_EXPANDED_HEIGHT,
  ONBOARDING_RESIZE_MS,
} from '@/lib/onboarding-transition';

const ONBOARDING_WINDOW_WIDTH = 600;
const MAIN_WINDOW_WIDTH = 1100;
const MAIN_WINDOW_HEIGHT = 700;
const MAIN_WINDOW_MIN_WIDTH = 680;
const WINDOW_SETTLE_BUFFER_MS = 34;

interface WindowResizeOptions {
  animated?: boolean;
}

function prefersReducedMotion(): boolean {
  return typeof globalThis.matchMedia === 'function'
    && globalThis.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

async function resizeCenteredWindow(
  width: number,
  height: number,
  animated: boolean,
): Promise<void> {
  const currentWindow = getCurrentWindow();

  if (!animated || prefersReducedMotion()) {
    await currentWindow.setSize(new LogicalSize(width, height));
    await currentWindow.center();
    return;
  }

  try {
    const durationMs = await invoke<number>('animate_main_window', {
      width,
      height,
      durationMs: ONBOARDING_RESIZE_MS,
    });
    await new Promise<void>((resolve) => {
      globalThis.setTimeout(resolve, durationMs + WINDOW_SETTLE_BUFFER_MS);
    });
  } catch (error) {
    console.warn('[onboarding] native window animation failed, using immediate resize:', error);
    await currentWindow.setSize(new LogicalSize(width, height));
    await currentWindow.center();
  }
}

export async function configureOnboardingWindow(
  height = ONBOARDING_EXPANDED_HEIGHT,
  { animated = false }: WindowResizeOptions = {},
): Promise<void> {
  if (!isTauri()) return;
  const currentWindow = getCurrentWindow();
  const size = new LogicalSize(ONBOARDING_WINDOW_WIDTH, height);

  // The main window starts with the narrowest supported sidebar + content width.
  // Clear those constraints before
  // shrinking it, then pin both bounds so macOS cannot restore a larger saved window size.
  await currentWindow.setMinSize(null);
  await currentWindow.setMaxSize(null);
  await currentWindow.setMaximizable(false);
  await currentWindow.setResizable(false);
  await resizeCenteredWindow(ONBOARDING_WINDOW_WIDTH, height, animated);
  await currentWindow.setMinSize(size);
  await currentWindow.setMaxSize(size);
}

export async function restoreMainWindow(
  { animated = false }: WindowResizeOptions = {},
): Promise<void> {
  if (!isTauri()) return;
  const currentWindow = getCurrentWindow();
  await currentWindow.setMinSize(null);
  await currentWindow.setMaxSize(null);
  await currentWindow.setMaximizable(false);
  await currentWindow.setResizable(false);
  await resizeCenteredWindow(MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT, animated);
  await currentWindow.setMinSize(new LogicalSize(MAIN_WINDOW_MIN_WIDTH, 1));
  await currentWindow.setMaximizable(true);
  await currentWindow.setResizable(true);
}
