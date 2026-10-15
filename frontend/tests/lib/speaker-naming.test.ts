import { describe, expect, test } from 'bun:test';
import {
  decideAutoGuessNames, formatNamingResult, hasUnnamedSpeaker, isWaitingForSpeakers,
} from '../../src/lib/speakerNaming';
import { makeSpeaker } from '../fixtures/speakers';

const unnamed = [makeSpeaker('spk_0', { display_name: 'Noah', name_source: 'user' }), makeSpeaker('spk_1')];
const rule = (overrides: Partial<Parameters<typeof decideAutoGuessNames>[0]> = {}) => decideAutoGuessNames({
  speakerIdentification: true,
  isAutoSummary: false,
  speakers: unnamed,
  ...overrides,
});

describe('automatic naming rule', () => {
  test('cloud is allowed only when auto-summary already sends the transcript there', () => {
    // The backend declines a cloud model unless the request allows it.
    expect(rule()).toEqual({ allowCloud: false });
    expect(rule({ isAutoSummary: true })).toEqual({ allowCloud: true });
  });

  test('nothing is requested with the beta off', () => {
    expect(rule({ speakerIdentification: false, isAutoSummary: true })).toBeNull();
  });

  test('nothing is requested when every speaker with rows has a name', () => {
    const named = [makeSpeaker('spk_0', { display_name: 'Noah' }), makeSpeaker('spk_1', { row_count: 0 })];
    expect(hasUnnamedSpeaker(named)).toBe(false);
    expect(hasUnnamedSpeaker(unnamed)).toBe(true);
    expect(rule({ speakers: named, isAutoSummary: true })).toBeNull();
  });
});

describe('naming result and summary wait', () => {
  test('result text', () => {
    expect(formatNamingResult(3, 2)).toBe('Named 3, suggested 2');
    expect(formatNamingResult(1, 0)).toBe('Named 1');
    expect(formatNamingResult(0, 2)).toBe('Suggested 2');
    expect(formatNamingResult(0, 0)).toBe('No names found');
  });

  test('the summary waits for the job and the pending automatic naming, up to the cap', () => {
    const base = { expired: false, statusKnown: true, isActive: false, autoNamingPending: false };
    expect(isWaitingForSpeakers(base)).toBe(false);
    expect(isWaitingForSpeakers({ ...base, statusKnown: false })).toBe(true);
    expect(isWaitingForSpeakers({ ...base, isActive: true })).toBe(true);
    expect(isWaitingForSpeakers({ ...base, autoNamingPending: true })).toBe(true);
    expect(isWaitingForSpeakers({ ...base, expired: true, isActive: true, autoNamingPending: true })).toBe(false);
  });
});
