import { afterAll, describe, expect, mock, test } from 'bun:test';
import type { ReactNode } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { MeetingSpeaker } from '../../src/types';
import { textOf } from '../fixtures/render';
import { makeSpeaker } from '../fixtures/speakers';

const originalPopover = { ...await import('../../src/components/ui/popover') };
const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('../../src/components/ui/popover', () => originalPopover);
  mock.module('sonner', () => originalToast);
});
// Render popover content inline so its buttons can be pressed without a DOM.
const passthrough = ({ children }: { children?: ReactNode }) => <>{children}</>;
mock.module('../../src/components/ui/popover', () => ({
  Popover: passthrough, PopoverTrigger: passthrough, PopoverContent: passthrough, PopoverAnchor: passthrough,
}));
mock.module('sonner', () => ({ toast: { error: () => {}, success: () => {}, info: () => {}, warning: () => {} } }));
const { SpeakerChip } = await import('../../src/components/Speakers/SpeakerChip');

const speakers: MeetingSpeaker[] = [
  makeSpeaker('spk_0', { speech_seconds: 6, row_count: 3, row_seconds: 6 }),
  makeSpeaker('spk_1', { display_name: 'Ana', speech_seconds: 2, row_count: 1, row_seconds: 2 }),
];
const autoNamed: MeetingSpeaker[] = [
  makeSpeaker('spk_0', { display_name: 'Noah', person_id: 'person-1', name_source: 'voice' }),
  speakers[1],
];
const suggested: MeetingSpeaker[] = [
  makeSpeaker('spk_0', {
    suggested_name: 'Ana', suggestion_source: 'conversation', suggestion_reason: 'addressed as Ana at 01:12',
  }),
  makeSpeaker('spk_1'),
];
const noop = async () => {};
const base = {
  transcriptId: 't2', editable: true, onRename: noop, onMerge: noop, onReassign: noop, onConfirm: noop, onReject: noop,
};

const buttons = (renderer: ReactTestRenderer, label: string) =>
  renderer.root.findAll((n) => n.type === 'button' && n.props['aria-label'] === label);
const button = (renderer: ReactTestRenderer, label: string) =>
  renderer.root.find((n) => n.type === 'button' && n.props['aria-label'] === label);

describe('compact speaker control', () => {
  test('reassigns the row it sits on, inside a same-speaker run', async () => {
    const onReassign = mock(async (_id: string, _key: string | null) => {});
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SpeakerChip {...base} compact speakerKey="spk_0" speakers={speakers} names={{ spk_1: 'Ana' }} onReassign={onReassign} />);
    });
    expect(buttons(renderer, 'Change speaker (Speaker 1)')).toHaveLength(1);
    await act(async () => { button(renderer, 'This line was said by Ana').props.onClick(); });
    expect(onReassign).toHaveBeenCalledWith('t2', 'spk_1');
  });

  test('offers no edit controls when editing is off', async () => {
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SpeakerChip {...base} editable={false} speakerKey="spk_0" speakers={speakers} names={{}} />);
    });
    expect(renderer.root.findAll((n) => n.type === 'button')).toHaveLength(0);
  });

  test('renames from the chip with the shared form', async () => {
    const onRename = mock(async (_key: string, _name: string) => {});
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SpeakerChip {...base} speakerKey="spk_1" speakers={speakers} names={{ spk_1: 'Ana' }} onRename={onRename} />);
    });
    const form = renderer.root.find((n) => n.type === 'form');
    await act(async () => { await form.props.onSubmit({ preventDefault: () => {} }); });
    expect(onRename).toHaveBeenCalledWith('spk_1', 'Ana');
  });
});

describe('chip name states', () => {
  test('auto name shows badge with confirm and reject', async () => {
    const onConfirm = mock(async (_key: string) => {});
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SpeakerChip {...base} speakerKey="spk_0" speakers={autoNamed} names={{ spk_0: 'Noah', spk_1: 'Ana' }} onConfirm={onConfirm} />);
    });
    expect(textOf(renderer.toJSON())).toContain('auto');
    expect(buttons(renderer, 'Not Noah')).toHaveLength(1);
    await act(async () => { button(renderer, 'Confirm Noah').props.onClick(); });
    expect(onConfirm).toHaveBeenCalledWith('spk_0');
  });

  test('suggestion shows question mark and actions', async () => {
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SpeakerChip {...base} speakerKey="spk_0" speakers={suggested} names={{ spk_0: 'Speaker 1' }} />);
    });
    expect(textOf(renderer.toJSON())).toContain('· Ana?');
    expect(renderer.root.findAll((n) => n.type === 'span' && n.props.title === 'addressed as Ana at 01:12')).toHaveLength(1);
    expect(buttons(renderer, 'Confirm Ana')).toHaveLength(1);
    expect(buttons(renderer, 'Not Ana')).toHaveLength(1);
  });

  test('reject calls on reject with the key', async () => {
    const onReject = mock(async (_key: string) => {});
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SpeakerChip {...base} speakerKey="spk_0" speakers={suggested} names={{}} onReject={onReject} />);
    });
    await act(async () => { button(renderer, 'Not Ana').props.onClick(); });
    expect(onReject).toHaveBeenCalledWith('spk_0');
  });

  test('read only chip shows no actions', async () => {
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SpeakerChip {...base} editable={false} speakerKey="spk_0" speakers={autoNamed} names={{ spk_0: 'Noah' }} />);
    });
    expect(textOf(renderer.toJSON())).toContain('auto');
    expect(renderer.root.findAll((n) => n.type === 'button')).toHaveLength(0);
  });
});

describe('speaker samples', () => {
  test('sample button plays the speaker', async () => {
    const onPlaySample = mock((_key: string) => {});
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SpeakerChip {...base} speakerKey="spk_1" speakers={speakers} names={{ spk_1: 'Ana' }} onPlaySample={onPlaySample} />);
    });
    await act(async () => { button(renderer, 'Play a sample of Ana').props.onClick(); });
    expect(onPlaySample).toHaveBeenCalledWith('spk_1');

    // The compact dot inside a run has no sample button.
    await act(async () => {
      renderer.update(<SpeakerChip {...base} compact speakerKey="spk_1" speakers={speakers} names={{ spk_1: 'Ana' }} onPlaySample={onPlaySample} />);
    });
    expect(buttons(renderer, 'Play a sample of Ana')).toHaveLength(0);
  });
});
