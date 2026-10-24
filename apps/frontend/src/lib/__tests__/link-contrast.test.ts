import { describe, expect, it } from 'vitest';

import { buttonVariants } from '@/components/ui/button';

import { ratio, token } from './design-token';

/**
 * リンク文字（text-link）と面の対比（WCAG 2.x の相対輝度）。ボタンの地に使う淡い accent
 * （--primary）をリンク文字にも使うと明るい面で約 1.8:1 になるため、リンクは --link に分けて
 * 明るい面でも暗い面でも 4.5:1 以上であることを縛る。
 */
describe('リンク文字の対比', () => {
  it.each([
    [':root', '明るい面'],
    ['.dark', '暗い面'],
  ])('%s（%s）の text-link は bg-background / bg-card 上で 4.5:1 以上', (scope) => {
    for (const ground of ['background', 'card']) {
      expect(ratio(token(scope, 'link'), token(scope, ground))).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('従来のリンク文字（text-primary）は明るい面で 4.5:1 に届かなかった（陽性対照）', () => {
    expect(ratio(token(':root', 'primary'), token(':root', 'background'))).toBeLessThan(4.5);
  });

  it('Button の link variant は text-link を使う', () => {
    expect(buttonVariants({ variant: 'link' }).split(' ')).toContain('text-link');
  });
});
