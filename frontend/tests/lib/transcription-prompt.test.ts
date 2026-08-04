import { describe, test, expect, beforeEach } from 'bun:test';

// Tests for transcription prompt and dictionary logic

function getEffectivePrompt(dictionary: string[], prompts: Record<string, string>, provider: string): string {
  const dictPart = dictionary.length > 0 ? dictionary.join(', ') + '.' : '';
  const modelPrompt = prompts[provider] || '';
  if (dictPart && modelPrompt) return `${dictPart} ${modelPrompt}`;
  return dictPart || modelPrompt;
}

describe('getEffectivePrompt', () => {
  test('dictionary only → joined with period', () => {
    const result = getEffectivePrompt(['Meetily', 'Whisper', 'VAD'], {}, 'localWhisper');
    expect(result).toBe('Meetily, Whisper, VAD.');
  });

  test('prompt only → prompt text unchanged', () => {
    const result = getEffectivePrompt([], { localWhisper: 'Technical meeting about AI' }, 'localWhisper');
    expect(result).toBe('Technical meeting about AI');
  });

  test('both dictionary and prompt → dictionary prepended with space', () => {
    const result = getEffectivePrompt(['Tauri', 'Rust'], { localWhisper: 'Desktop app discussion' }, 'localWhisper');
    expect(result).toBe('Tauri, Rust. Desktop app discussion');
  });

  test('neither → empty string', () => {
    const result = getEffectivePrompt([], {}, 'localWhisper');
    expect(result).toBe('');
  });

  test('uses correct provider key', () => {
    const result = getEffectivePrompt([], { localWhisper: 'whisper prompt', groq: 'groq prompt' }, 'groq');
    expect(result).toBe('groq prompt');
  });

  test('missing provider key returns empty string', () => {
    const result = getEffectivePrompt([], { localWhisper: 'only whisper' }, 'groq');
    expect(result).toBe('');
  });
});

describe('localStorage round-trip', () => {
  const mockStorage: Record<string, string> = {};
  const mockLocalStorage = {
    getItem: (key: string) => mockStorage[key] ?? null,
    setItem: (key: string, value: string) => { mockStorage[key] = value; },
    removeItem: (key: string) => { delete mockStorage[key]; },
    clear: () => { Object.keys(mockStorage).forEach(k => delete mockStorage[k]); },
  };

  beforeEach(() => { mockLocalStorage.clear(); });

  test('dictionary persists as JSON array', () => {
    const terms = ['Meetily', 'Whisper', 'CUDA'];
    mockLocalStorage.setItem('transcriptionDictionary', JSON.stringify(terms));
    const loaded = JSON.parse(mockLocalStorage.getItem('transcriptionDictionary') ?? '[]');
    expect(loaded).toEqual(terms);
  });

  test('prompt persists per provider', () => {
    const prompts = { localWhisper: 'My prompt', groq: 'Groq prompt' };
    mockLocalStorage.setItem('transcriptionPrompts', JSON.stringify(prompts));
    const loaded = JSON.parse(mockLocalStorage.getItem('transcriptionPrompts') ?? '{}');
    expect(loaded['localWhisper']).toBe('My prompt');
    expect(loaded['groq']).toBe('Groq prompt');
  });

  test('missing key returns empty defaults gracefully', () => {
    const dict = JSON.parse(mockLocalStorage.getItem('transcriptionDictionary') ?? '[]');
    const prompts = JSON.parse(mockLocalStorage.getItem('transcriptionPrompts') ?? '{}');
    expect(dict).toEqual([]);
    expect(prompts).toEqual({});
  });
});
