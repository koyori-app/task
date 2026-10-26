import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

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

  it('src に --primary を文字色・輪の色に読む箇所が無い', () => {
    const root = resolve(__dirname, '../..');
    const offenders = readdirSync(root, { recursive: true, encoding: 'utf8' })
      .filter((file) => /\.(vue|ts|css)$/.test(file) && !file.includes('__tests__'))
      .flatMap((file) =>
        readFileSync(resolve(root, file), 'utf8')
          .split('\n')
          .map((line, i) => [`${file}:${i + 1}`, line] as const)
          .filter(([, line]) => PRIMARY_AS_FOREGROUND.some((re) => re.test(line))),
      )
      .map(([where]) => where);
    expect(offenders).toEqual([]);
  });
});

/** `text-primary` の class と、`color` / `outline` の値に `var(--primary)` を読む CSS・JS（地の色は対象外）。 */
const PRIMARY_AS_FOREGROUND = [
  /\btext-primary(?!-foreground)\b/,
  /(?:^|[\s{;,])(?:color|outline)\s*:\s*['"]?[^;'"]*var\(--primary\)/,
];
