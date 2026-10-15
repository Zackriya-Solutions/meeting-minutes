import { describe, expect, test } from 'bun:test';
import { renderToStaticMarkup } from 'react-dom/server';
import { SummaryContextBox } from '../../src/components/MeetingDetails/SummaryContextBox';
import type { InheritedContext } from '../../src/lib/summary-context';

const global: InheritedContext = { scope: 'global', scopeId: '', label: null, context: 'I am Noah.' };
const folder: InheritedContext = { scope: 'folder', scopeId: 'f1', label: 'Vendor sync', context: 'Weekly sync.' };

function render(overrides: Partial<Parameters<typeof SummaryContextBox>[0]> = {}) {
  return renderToStaticMarkup(
    <SummaryContextBox
      text=""
      onTextChange={() => {}}
      useInherited
      onUseInheritedChange={() => {}}
      inherited={[]}
      isLoaded
      onEditInherited={() => {}}
      {...overrides}
    />,
  );
}

describe('summary context box', () => {
  test('shows no inherited line when nothing is inherited', () => {
    const html = render();
    expect(html).not.toContain('Also sent:');
    expect(html).not.toContain('Use inherited');
    expect(html).toContain('<textarea');
  });

  test('names each inherited level and offers the switch', () => {
    const html = render({ inherited: [global, folder] });
    expect(html).toContain('Also sent:');
    expect(html).toContain('Global context');
    expect(html).toContain('Folder &quot;Vendor sync&quot;');
    expect(html).toContain('Use inherited');
    expect(html).not.toContain('text-gray-400');
  });

  test('greys the inherited names out when the switch is off', () => {
    expect(render({ inherited: [global], useInherited: false })).toContain('text-gray-400');
  });

  test('the text area is disabled until the saved text has loaded', () => {
    expect(render({ isLoaded: false })).toContain('disabled=""');
    expect(render({ isLoaded: true })).not.toContain('disabled=""');
  });

  test('the text area enforces the limit and the counter appears near it', () => {
    expect(render().toLowerCase()).toContain('maxlength="4000"');
    expect(render({ text: 'x'.repeat(3599) })).not.toContain('/ 4,000');
    expect(render({ text: 'x'.repeat(3600) })).toContain('3,600 / 4,000');
  });
});
