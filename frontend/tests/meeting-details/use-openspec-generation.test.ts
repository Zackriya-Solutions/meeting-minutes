import { describe, expect, mock, test } from 'bun:test';
import { renderToStaticMarkup } from 'react-dom/server';
import {
  advanceOpenSpecState,
  generateOpenSpecBundle,
} from '../../src/hooks/meeting-details/useOpenSpecGeneration';
import { OpenSpecGeneratorButtonGroupView } from '../../src/components/MeetingDetails/OpenSpecGeneratorButtonGroup';

describe('useOpenSpecGeneration state transitions', () => {
  test('idle -> generating -> done', () => {
    let state = advanceOpenSpecState('idle', 'start');
    expect(state).toBe('generating');

    state = advanceOpenSpecState(state, 'success');
    expect(state).toBe('done');
  });

  test('generating -> error -> idle on retry reset', () => {
    let state = advanceOpenSpecState('generating', 'failure');
    expect(state).toBe('error');

    state = advanceOpenSpecState(state, 'reset_error');
    expect(state).toBe('idle');
  });

  test('done -> start (regenerate) -> generating', () => {
    const state = advanceOpenSpecState('done', 'start');
    expect(state).toBe('generating');
  });
});

describe('OpenSpecGeneratorButtonGroup', () => {
  test('renders when transcript is present', () => {
    const html = renderToStaticMarkup(
      OpenSpecGeneratorButtonGroupView({
        hasTranscripts: true,
        status: 'idle',
        onGenerate: async () => {},
        onRegenerate: async () => {},
        t: (key: string) => key,
      })
    );

    expect(html).toContain('openspec.generate');
    expect(html).toContain('button');
  });

  test('is hidden when transcript is missing', () => {
    const rendered = OpenSpecGeneratorButtonGroupView({
      hasTranscripts: false,
      status: 'idle',
      onGenerate: async () => {},
      onRegenerate: async () => {},
      t: (key: string) => key,
    });

    expect(rendered).toBeNull();
  });

  test('done state click routes to regenerate handler', () => {
    let generatedCalls = 0;
    let regeneratedCalls = 0;

    const rendered = OpenSpecGeneratorButtonGroupView({
      hasTranscripts: true,
      status: 'done',
      onGenerate: async () => {
        generatedCalls += 1;
      },
      onRegenerate: async () => {
        regeneratedCalls += 1;
      },
      t: (key: string) => key,
    }) as any;

    const buttonElement = rendered.props.children;
    buttonElement.props.onClick();

    expect(regeneratedCalls).toBe(1);
    expect(generatedCalls).toBe(0);
  });
});

describe('generateOpenSpecBundle runtime flow', () => {
  test('calls save-as API after successful generation', async () => {
    const calls: Array<{ cmd: string; args?: Record<string, unknown> }> = [];

    const invokeFn = mock(async (command: string, args?: Record<string, unknown>) => {
      calls.push({ cmd: command, args });
      if (command === 'api_generate_openspec_bundle') {
        return {
          type: 'success',
          zip_temp_path: '/tmp/demo.zip',
          suggested_filename: 'demo.zip',
          slug: 'demo',
        };
      }

      if (command === 'api_save_openspec_bundle_as') {
        return { cancelled: false, saved_path: '/tmp/exported.zip' };
      }

      throw new Error(`unexpected command: ${command}`);
    });

    const result = await generateOpenSpecBundle(
      { meetingId: 'meeting-1', hasTranscript: true },
      {
        invokeFn,
        t: (key: string) => key,
        showToastError: () => {},
        showToastSuccess: () => {},
        showToastInfo: () => {},
      }
    );

    expect(result.state).toBe('done');
    expect(result.error).toBeNull();
    expect(calls.map(call => call.cmd)).toEqual([
      'api_generate_openspec_bundle',
      'api_save_openspec_bundle_as',
    ]);
    expect(calls[1]?.args).toEqual({
      zipTempPath: '/tmp/demo.zip',
      suggestedFilename: 'demo.zip',
    });
  });
});
