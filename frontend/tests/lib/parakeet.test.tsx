import { describe, expect, test } from 'bun:test';
import React from 'react';
import ReactDOMServer from 'react-dom/server';
import { ModelCard } from '../../src/components/ParakeetModelManager';
import { ParakeetModelInfo, ModelStatus } from '../../src/lib/parakeet';

function makeModel(status: ModelStatus): ParakeetModelInfo {
  return {
    name: 'parakeet-tdt-0.6b-v3-int8',
    path: '/models/parakeet-tdt-0.6b-v3-int8',
    size_mb: 670,
    accuracy: 'High',
    speed: 'Ultra Fast',
    status,
    quantization: 'Int8',
  };
}

describe('Parakeet model manager', () => {
  test('ModelCard renders a real percent from a nested Downloading status', () => {
    const model = makeModel({ Downloading: { progress: 42 } });

    const html = ReactDOMServer.renderToStaticMarkup(
      <ModelCard
        model={model}
        isSelected={false}
        isRecommended={false}
        onSelect={() => {}}
        onDownload={() => {}}
        onCancel={() => {}}
        onDelete={() => {}}
        isDownloading={true}
        isCancelling={false}
      />
    );

    expect(html).toContain('42%');
    expect(html).not.toContain('NaN%');
  });
});
