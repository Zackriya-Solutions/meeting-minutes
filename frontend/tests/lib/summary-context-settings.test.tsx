import { describe, expect, test } from 'bun:test';
import { renderToStaticMarkup } from 'react-dom/server';
import { SummaryContextSettings } from '../../src/components/SummaryContextSettings';

describe('global summary context settings', () => {
  test('explains what the text is for and starts disabled until it has loaded', () => {
    const html = renderToStaticMarkup(<SummaryContextSettings />);
    expect(html).toContain('Summary Context');
    expect(html).toContain(
      'Sent with every summary. Use it for things that are always true: who you are, your organization, names, and terms.',
    );
    expect(html.toLowerCase()).toContain('maxlength="4000"');
    expect(html).toContain('disabled=""');
  });
});
