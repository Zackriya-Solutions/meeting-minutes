import { describe, expect, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { SpeakerJobBanner } from '../../src/components/Speakers/SpeakerJobBanner';
import type { SpeakerJobStatus } from '../../src/types';
import { textOf } from '../fixtures/render';

async function bannerText(job: SpeakerJobStatus): Promise<string> {
  let renderer!: ReactTestRenderer;
  await act(async () => { renderer = create(<SpeakerJobBanner job={job} onCancel={() => {}} />); });
  const text = textOf(renderer.toJSON());
  await act(async () => renderer.unmount());
  return text;
}
const job = (overrides: Partial<SpeakerJobStatus>): SpeakerJobStatus => ({
  meeting_id: 'meeting-a', kind: 'identify', state: 'queued', stage: null, percent: 0, message: 'Waiting to start…', ...overrides,
});

describe('speaker job banner', () => {
  test('a queued naming job waits to find names', async () => {
    const text = await bannerText(job({ kind: 'naming' }));
    expect(text).toContain('Waiting to find names…');
    expect(text).not.toContain('identify');
  });

  test('a queued identify job waits to identify speakers', async () => {
    expect(await bannerText(job({ kind: 'identify' }))).toContain('Waiting to identify speakers…');
  });

  test('a running job shows its own message', async () => {
    const text = await bannerText(job({ kind: 'naming', state: 'running', stage: 'naming', percent: 10, message: 'Finding names…' }));
    expect(text).toContain('Finding names…');
  });
});
