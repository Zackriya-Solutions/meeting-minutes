import type { ReactTestRendererJSON } from 'react-test-renderer';

/** All text a rendered tree shows, concatenated in order. */
export function textOf(node: ReactTestRendererJSON | ReactTestRendererJSON[] | string | null): string {
  if (node === null) return '';
  if (typeof node === 'string') return node;
  if (Array.isArray(node)) return node.map(textOf).join('');
  return (node.children ?? []).map(textOf).join('');
}
