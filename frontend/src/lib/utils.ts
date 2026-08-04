import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/**
 * Detects if an error message indicates that Ollama is not installed or not running
 * @param errorMessage - The error message to check
 * @returns true if the error indicates Ollama is not installed/running
 */
export function isOllamaNotInstalledError(errorMessage: string): boolean {
  if (!errorMessage) return false;

  const lowerError = errorMessage.toLowerCase();

  // Check for common patterns that indicate Ollama is not installed or not running
  const patterns = [
    'cannot connect',
    'connection refused',
    'cli not found',
    'not in path',
    'ollama cli not found',
    'not found or not in path',
    'please check if the server is running',
    'please check if the ollama server is running',
    'econnrefused',
  ];

  return patterns.some(pattern => lowerError.includes(pattern));
}

/**
 * Detects if an OpenSpec generation error means Node.js/OpenSpec runtime is missing.
 */
export function isOpenSpecDependencyError(errorCode?: string, errorMessage?: string): boolean {
  const normalizedCode = (errorCode || '').toLowerCase();
  if (normalizedCode === 'node_missing' || normalizedCode === 'cli_missing') return true;

  const lowerError = (errorMessage || '').toLowerCase();
  const patterns = [
    'node.js is required',
    'npx',
    'openspec cli not found',
    'neither global openspec nor npx',
  ];

  return patterns.some(pattern => lowerError.includes(pattern));
}

export function isOpenSpecNetworkError(errorCode?: string, errorMessage?: string): boolean {
  if ((errorCode || '').toLowerCase() === 'network_unavailable') return true;
  const lower = (errorMessage || '').toLowerCase();
  return [
    'network',
    'registry.npmjs.org',
    'enotfound',
    'eai_again',
    'fetch failed',
  ].some(pattern => lower.includes(pattern));
}

export function isOpenSpecTimeoutError(errorCode?: string, errorMessage?: string): boolean {
  if ((errorCode || '').toLowerCase() === 'timeout') return true;
  return (errorMessage || '').toLowerCase().includes('timed out');
}
