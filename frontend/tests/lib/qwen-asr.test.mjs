import { describe, expect, test } from 'bun:test';
import {
  QWEN3_ASR_1_7B_MODEL,
  QWEN3_ASR_MODEL,
  QWEN_MODEL_DISPLAY,
} from '../../src/lib/qwenAsr';

describe('Qwen3-ASR model metadata', () => {
  test('keeps 0.6B as the recommended compact model', () => {
    expect(QWEN_MODEL_DISPLAY[QWEN3_ASR_MODEL]).toMatchObject({
      name: 'Qwen3-ASR 0.6B Int8',
      icon: '🌐',
      recommended: true,
    });
  });

  test('exposes 1.7B as a separate higher-capacity model', () => {
    expect(QWEN_MODEL_DISPLAY[QWEN3_ASR_1_7B_MODEL]).toMatchObject({
      name: 'Qwen3-ASR 1.7B Int8',
      icon: '🧠',
    });
    expect(QWEN3_ASR_1_7B_MODEL).not.toBe(QWEN3_ASR_MODEL);
  });
});
