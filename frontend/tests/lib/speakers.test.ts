import { describe, expect, test } from 'bun:test';
import {
  buildSpeakerNameMap, defaultSpeakerLabel, formatPropagationMessage, formatSpeakerCount,
  isSpeakerRunStart, rowSpeakerControl, speakerNameState,
} from '../../src/lib/speakers';
import type { MeetingSpeaker } from '../../src/types';
import { makeSpeaker } from '../fixtures/speakers';

const speaker = (speaker_key: string, display_name: string | null): MeetingSpeaker => makeSpeaker(speaker_key, { display_name });

describe('speaker helpers', () => {
  test('default labels are one-based', () => {
    expect(defaultSpeakerLabel('spk_0')).toBe('Speaker 1');
    expect(defaultSpeakerLabel('spk_4')).toBe('Speaker 5');
    expect(defaultSpeakerLabel('weird')).toBe('weird');
  });

  test('blank names fall back to the default label', () => {
    expect(buildSpeakerNameMap([speaker('spk_0', '  '), speaker('spk_1', 'Ana')])).toEqual({ spk_0: 'Speaker 1', spk_1: 'Ana' });
  });

  test('only the first row of a same-speaker run starts a run', () => {
    const rows = [{ speaker: 'spk_0' }, { speaker: 'spk_0' }, { speaker: 'spk_0' }, { speaker: null }, { speaker: 'spk_1' }];
    expect(rows.map((_, i) => isSpeakerRunStart(rows, i))).toEqual([true, false, false, false, true]);
  });

  test('rows inside a run get the compact control only while editing is possible', () => {
    expect(rowSpeakerControl('spk_0', true, false)).toEqual({ speakerKey: 'spk_0', compact: false });
    expect(rowSpeakerControl('spk_0', false, true)).toEqual({ speakerKey: 'spk_0', compact: true });
    expect(rowSpeakerControl('spk_0', false, false)).toBeNull();
    expect(rowSpeakerControl(null, true, true)).toBeNull();
  });

  test('speaker counts are pluralised', () => {
    expect(formatSpeakerCount(1)).toBe('1 speaker');
    expect(formatSpeakerCount(3)).toBe('3 speakers');
  });
});

describe('propagation toast', () => {
  test('formatPropagationMessage is singular for one meeting and plural otherwise', () => {
    expect(formatPropagationMessage(1)).toBe('Also named in 1 other meeting');
    expect(formatPropagationMessage(3)).toBe('Also named in 3 other meetings');
  });
});

describe('speaker name state', () => {
  test('a missing or unnamed speaker shows the default label', () => {
    expect(speakerNameState(undefined)).toEqual({ kind: 'default' });
    expect(speakerNameState(makeSpeaker('spk_0'))).toEqual({ kind: 'default' });
  });

  test('a typed name is named', () => {
    expect(speakerNameState(makeSpeaker('spk_0', { display_name: 'Noah', person_id: 'p1', name_source: 'user' })))
      .toEqual({ kind: 'named', name: 'Noah' });
  });

  test('a name from before people existed counts as typed', () => {
    expect(speakerNameState(makeSpeaker('spk_0', { display_name: ' Noah ', name_source: null })))
      .toEqual({ kind: 'named', name: 'Noah' });
  });

  test('voice and conversation names are auto', () => {
    expect(speakerNameState(makeSpeaker('spk_0', { display_name: 'Noah', person_id: 'p1', name_source: 'voice' })))
      .toEqual({ kind: 'auto', name: 'Noah', source: 'voice' });
    expect(speakerNameState(makeSpeaker('spk_0', { display_name: 'Ana', name_source: 'conversation' })))
      .toEqual({ kind: 'auto', name: 'Ana', source: 'conversation' });
  });

  test('an unnamed speaker with a suggestion shows it with its reason', () => {
    expect(speakerNameState(makeSpeaker('spk_1', {
      display_name: '  ', suggested_name: 'Ana', suggested_person_id: 'p2', suggestion_source: 'voice', suggestion_reason: 'voice match 0.68',
    }))).toEqual({ kind: 'suggestion', name: 'Ana', reason: 'voice match 0.68', source: 'voice' });
  });
});
