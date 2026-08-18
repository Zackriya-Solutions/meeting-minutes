import { describe, expect, test } from 'bun:test';
import {
  extractSummaryAgreementRows,
  extractSummaryAgreements,
} from '../../src/lib/summaryToMarkdown';

describe('extractSummaryAgreements', () => {
  test('renders one agreement without a bullet', () => {
    expect(extractSummaryAgreements(`
## Договорённости
- Анна подготовит релиз к пятнице.
`)).toBe('Анна подготовит релиз к пятнице.');
  });

  test('keeps bullets when there are multiple agreements', () => {
    expect(extractSummaryAgreements(`
## Договорённости
- Анна подготовит релиз к пятнице.
- Игорь проверит сборку сегодня.
`)).toBe('- Анна подготовит релиз к пятнице.\n- Игорь проверит сборку сегодня.');
  });
});

describe('extractSummaryAgreementRows', () => {
  test('keeps recognized owners first and clears unknown metadata', () => {
    const rows = extractSummaryAgreementRows(`
## Договорённости
- Решено выпустить обновление.

## Задачи
| Ответственный | Задача | Срок | Фрагмент |
| --- | --- | --- | --- |
| — | Проверить сборку | не указан | текст |
| Анна | Подготовить релиз | Завтра | текст |
`);

    expect(rows).toEqual([
      { who: 'Анна', what: 'Подготовить релиз', when: 'Завтра' },
      { who: '', what: 'Проверить сборку', when: '' },
      { who: '', what: 'Решено выпустить обновление.', when: '' },
    ]);
  });

  test('reads a three-column agreements table', () => {
    const rows = extractSummaryAgreementRows(`
## Договорённости
| Кто | Что | Когда |
| --- | --- | --- |
| Спикер 2 | Отправить макет | Пятница |
| unknown | Проверить аналитику | not stated |
`);

    expect(rows).toEqual([
      { who: 'Спикер 2', what: 'Отправить макет', when: 'Пятница' },
      { who: '', what: 'Проверить аналитику', when: '' },
    ]);
  });

  test('keeps legacy bullet agreements as actions with empty cells', () => {
    expect(extractSummaryAgreementRows(`
## Agreements
- Keep the current model.
`)).toEqual([
      { who: '', what: 'Keep the current model.', when: '' },
    ]);
  });
});
