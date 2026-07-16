import { describe, it, expect } from 'vitest';
import { MockDigestGenerator } from '@/valueos/digest/digest';

describe('MockDigestGenerator (readable recap, not a hash)', () => {
  it('produces readable prose with a title and key points', async () => {
    const g = new MockDigestGenerator();
    const out = await g.generate('We discussed pricing. Ada wants a Q3 pilot. Next step is a demo.', {
      title: 'Discovery call',
    });
    expect(out).toContain('Discovery call');
    expect(out).toMatch(/Key points/i);
    expect(out).toContain('pricing');
    // not a hash: contains spaces + real words
    expect(out).toMatch(/\s/);
    expect(out.length).toBeGreaterThan(20);
  });

  it('handles an empty transcript gracefully', async () => {
    const out = await new MockDigestGenerator().generate('   ');
    expect(out).toMatch(/no speech/i);
  });

  it('respects maxChars', async () => {
    const long = 'word '.repeat(5000);
    const out = await new MockDigestGenerator().generate(long, { maxChars: 200 });
    expect(out.length).toBeLessThanOrEqual(200);
  });
});
