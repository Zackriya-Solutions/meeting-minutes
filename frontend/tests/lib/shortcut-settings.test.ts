import { describe, test, expect } from 'bun:test';
import { validateShortcut, formatShortcut, DEFAULT_RECORDING_SHORTCUT } from '../../src/lib/shortcutUtils';

describe('validateShortcut', () => {
  test('valid shortcut with modifier and key', () => {
    expect(validateShortcut('Control+F8')).toBe(true);
  });

  test('valid shortcut multiple modifiers', () => {
    expect(validateShortcut('Control+Shift+R')).toBe(true);
  });

  test('rejects shortcut with no modifier', () => {
    expect(validateShortcut('F8')).toBe(false);
  });

  test('rejects empty string', () => {
    expect(validateShortcut('')).toBe(false);
  });

  test('rejects shortcut where last part is a modifier', () => {
    expect(validateShortcut('Control+Shift')).toBe(false);
  });

  test('valid shortcut with alt modifier', () => {
    expect(validateShortcut('Alt+Space')).toBe(true);
  });
});

describe('formatShortcut', () => {
  test('formats Control as Ctrl', () => {
    expect(formatShortcut('Control+F8')).toBe('Ctrl+F8');
  });

  test('formats Meta as ⌘', () => {
    expect(formatShortcut('Meta+K')).toBe('⌘+K');
  });

  test('uppercase single char key', () => {
    expect(formatShortcut('Control+a')).toBe('Ctrl+A');
  });

  test('empty string returns empty', () => {
    expect(formatShortcut('')).toBe('');
  });
});

describe('DEFAULT_RECORDING_SHORTCUT', () => {
  test('is Control+F8', () => {
    expect(DEFAULT_RECORDING_SHORTCUT).toBe('Control+F8');
  });

  test('is valid', () => {
    expect(validateShortcut(DEFAULT_RECORDING_SHORTCUT)).toBe(true);
  });
});
