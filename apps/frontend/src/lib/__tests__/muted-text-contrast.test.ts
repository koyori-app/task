import { describe, expect, it } from 'vitest';

import { ratio, token, type ThemeName } from './design-token';

/**
 * `bg-muted` の上の `text-muted-foreground`。文字なので 4.5:1 以上が要る（WCAG 1.4.3）。
 * 値は @koyori-app/ui-vue の配色から来る。余裕は小さいので、配色の値が変わったときに落とす。
 */
describe('bg-muted の上の text-muted-foreground', () => {
  it.each<ThemeName>(['light', 'dark'])('%s の配色で 4.5:1 以上', (theme) => {
    expect(ratio(token(theme, 'muted-foreground'), token(theme, 'muted'))).toBeGreaterThanOrEqual(
      4.5,
    );
  });
});
