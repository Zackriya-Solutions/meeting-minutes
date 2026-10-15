import { describe, expect, test } from 'bun:test';
import {
  inheritedContextKey,
  inheritedContextName,
  summaryContextCounter,
  type InheritedContext,
} from '../../src/lib/summary-context';

const level = (overrides: Partial<InheritedContext> = {}): InheritedContext => ({
  scope: 'global', scopeId: '', label: null, context: 'text', ...overrides,
});

describe('summary context display helpers', () => {
  test('names each inherited level', () => {
    expect(inheritedContextName(level())).toBe('Global context');
    expect(inheritedContextName(level({ scope: 'folder', scopeId: 'f1', label: 'Vendor sync' })))
      .toBe('Folder "Vendor sync"');
    expect(inheritedContextName(level({ scope: 'folder', scopeId: 'f1' }))).toBe('Folder context');
  });

  test('keys are unique per level', () => {
    expect(inheritedContextKey(level())).toBe('global:');
    expect(inheritedContextKey(level({ scope: 'folder', scopeId: 'f1' }))).toBe('folder:f1');
  });

  test('the counter appears only near the limit', () => {
    expect(summaryContextCounter(0)).toBeNull();
    expect(summaryContextCounter(3599)).toBeNull();
    expect(summaryContextCounter(3600)).toBe('3,600 / 4,000');
    expect(summaryContextCounter(4000)).toBe('4,000 / 4,000');
  });
});
