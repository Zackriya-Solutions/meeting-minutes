import { afterAll, describe, expect, mock, test } from 'bun:test';
import { Play } from 'lucide-react';
import type { ReactNode } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { TranscriptSegmentData } from '../../src/types';

const originalTooltip = { ...await import('../../src/components/ui/tooltip') };
afterAll(() => mock.module('../../src/components/ui/tooltip', () => originalTooltip));
// Radix tooltips need a provider; the rows only need their children.
const passthrough = ({ children }: { children?: ReactNode }) => <>{children}</>;
mock.module('../../src/components/ui/tooltip', () => ({
  ...originalTooltip, Tooltip: passthrough, TooltipTrigger: passthrough, TooltipContent: passthrough,
}));
const { VirtualizedTranscriptView } = await import('../../src/components/VirtualizedTranscriptView');
type RenderSpeaker = NonNullable<Parameters<typeof VirtualizedTranscriptView>[0]['renderSpeaker']>;

const segments: TranscriptSegmentData[] = [
  { id: 't1', timestamp: 0, text: 'hello', speaker: 'spk_0' },
  { id: 't2', timestamp: 2, text: 'again', speaker: 'spk_0' },
  { id: 't3', timestamp: 4, text: 'reply', speaker: 'spk_1' },
  { id: 't4', timestamp: 6, text: 'unlabelled', speaker: null },
];

describe('transcript rows', () => {
  test('re-render only when what they show changes', async () => {
    const calls: Array<[string | null, string, boolean]> = [];
    const renderSpeaker: RenderSpeaker = (key, id, runStart) => {
      calls.push([key, id, runStart]);
      return key ? <span>{key}</span> : null;
    };
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<VirtualizedTranscriptView segments={segments} renderSpeaker={renderSpeaker} disableAutoScroll totalCount={4} />);
    });
    expect(calls).toEqual([
      ['spk_0', 't1', true],
      ['spk_0', 't2', false],
      ['spk_1', 't3', true],
      [null, 't4', false],
    ]);

    // A parent re-render with the same rows and the same renderer (a progress event, a scroll) leaves rows alone.
    calls.length = 0;
    await act(async () => {
      renderer.update(<VirtualizedTranscriptView segments={segments} renderSpeaker={renderSpeaker} disableAutoScroll totalCount={5} />);
    });
    expect(calls).toEqual([]);

    // A new renderer (speakers renamed, editing toggled) reaches every row.
    await act(async () => {
      renderer.update(<VirtualizedTranscriptView segments={segments} renderSpeaker={(...args) => renderSpeaker(...args)} disableAutoScroll totalCount={5} />);
    });
    expect(calls).toHaveLength(4);
  });
});

describe('play from a line', () => {
  test('clicking timestamp plays from row start', async () => {
    const onPlayFrom = mock((_startS: number) => {});
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<VirtualizedTranscriptView segments={segments} onPlayFrom={onPlayFrom} disableAutoScroll totalCount={4} />);
    });
    const play = renderer.root.find((n) => n.type === 'button' && n.props['aria-label'] === 'Play from 00:02');
    await act(async () => { play.props.onClick(); });
    expect(onPlayFrom).toHaveBeenCalledWith(2);
  });

  test('the play icon sits inside the timestamp button, after the time', async () => {
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<VirtualizedTranscriptView segments={segments} onPlayFrom={() => {}} disableAutoScroll totalCount={4} />);
    });
    const play = renderer.root.find((n) => n.type === 'button' && n.props['aria-label'] === 'Play from 00:02');
    const parts = play.children.filter((c) => typeof c !== 'string' || c.trim() !== '');
    expect(parts[0]).toBe('[00:02]');
    expect(play.findAllByType(Play)).toHaveLength(1);
  });

  test('only rows whose active state changes rerender', async () => {
    const calls: string[] = [];
    const renderSpeaker: RenderSpeaker = (_key, id) => {
      calls.push(id);
      return null;
    };
    const onPlayFrom = () => {};
    const view = (activeSegmentId: string) => (
      <VirtualizedTranscriptView segments={segments} renderSpeaker={renderSpeaker} onPlayFrom={onPlayFrom}
        activeSegmentId={activeSegmentId} disableAutoScroll totalCount={4} />
    );
    let renderer!: ReactTestRenderer;
    const activeRows = () => renderer.root
      .findAll((n) => n.type === 'div' && n.props['aria-current'] === 'true')
      .map((n) => n.props.id);
    await act(async () => { renderer = create(view('t1')); });
    expect(activeRows()).toEqual(['segment-t1']);
    calls.length = 0;
    await act(async () => { renderer.update(view('t2')); });
    expect(calls.sort()).toEqual(['t1', 't2']);
    expect(activeRows()).toEqual(['segment-t2']);
  });

  test('rows without onPlayFrom render plain timestamps', async () => {
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<VirtualizedTranscriptView segments={segments} disableAutoScroll totalCount={4} />);
    });
    expect(renderer.root.findAll((n) => n.type === 'button' && String(n.props['aria-label']).startsWith('Play from'))).toHaveLength(0);
    expect(renderer.root.findAll((n) => n.type === 'span' && n.props.children === '[00:02]')).toHaveLength(1);
  });
});
