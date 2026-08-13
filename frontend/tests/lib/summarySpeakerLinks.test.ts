import { describe, expect, test } from 'bun:test';
import type { SpeakerInfo } from '../../src/types';
import {
  linkAgreementSpeakers,
  normalizedSpeakerName,
  roleForSpeaker,
} from '../../src/lib/summarySpeakerLinks';

const translate = (value: string) => value;

function speaker(
  id: number,
  displayName: string,
  aliases: string[] = [],
): SpeakerInfo {
  return {
    id,
    display_name: displayName,
    aliases,
    is_confirmed: true,
    is_self: false,
    segment_count: 1,
    speech_duration_seconds: 1,
  };
}

describe('summary speaker links', () => {
  test('replaces a persisted old alias with the current display name and stable id', () => {
    const result = linkAgreementSpeakers(
      '- Блин завтра сделает концепт.\n- Асинхронный енот пришлёт материалы.',
      [
        speaker(162, 'Зуйзуй', ['Блин']),
        speaker(163, 'Асинхронный енот'),
      ],
      translate,
    );

    expect(result).toBe(
      '- [Зуйзуй](#speaker-162) завтра сделает концепт.\n'
      + '- [Асинхронный енот](#speaker-163) пришлёт материалы.',
    );
  });

  test('also replaces a bold old name without leaving markdown markers behind', () => {
    expect(linkAgreementSpeakers(
      '- **Блин** изменит правила.',
      [speaker(162, 'Зуйзуй', ['Блин'])],
      translate,
    )).toBe('- [Зуйзуй](#speaker-162) изменит правила.');
  });

  test('does not link an alias shared by more than one speaker', () => {
    const source = '- Саша подготовит отчёт.';
    expect(linkAgreementSpeakers(
      source,
      [speaker(1, 'Александр', ['Саша']), speaker(2, 'Александра', ['Саша'])],
      translate,
    )).toBe(source);
  });

  test('resolves a persisted role through an old alias', () => {
    const renamed = speaker(162, 'Зуйзуй', ['Блин']);
    const roles = new Map([[normalizedSpeakerName('Блин'), 'Дизайнер']]);
    expect(roleForSpeaker(renamed, roles)).toBe('Дизайнер');
  });
});
