import { describe, it, expect } from 'vitest';
import {
  CUSTOM_FIELD_TYPES,
  customFieldTypeMeta,
  parseSelectOptions,
  serializeSelectOptions,
} from '../custom-field-types';
import type { CustomFieldType } from '../custom-field-types';

describe('CUSTOM_FIELD_TYPES', () => {
  // enum との過不足は custom-field-types.ts の `satisfies Record<CustomFieldType, …>` が
  // コンパイル時に保証する（値追加/削除で型エラー）。ここでは各型が meta とアイコンを
  // 持ち、CustomFieldType の全メンバーを網羅していることを実行時にも確認する。
  it('全メンバーが meta（ラベル・アイコン）を持ち、重複がない', () => {
    // CustomFieldType のユニオンを網羅した参照リスト。
    // openapi の enum に値が増えると、この Record のキー不足で型エラーになる
    const expectedKeys = {
      text: true,
      number: true,
      select: true,
      date: true,
      url: true,
      checkbox: true,
    } satisfies Record<CustomFieldType, true>;

    const values = CUSTOM_FIELD_TYPES.map((t) => t.value);
    expect(new Set(values).size).toBe(values.length); // 重複なし
    expect(new Set(values)).toEqual(new Set(Object.keys(expectedKeys)));
    for (const type of CUSTOM_FIELD_TYPES) {
      expect(type.label.length).toBeGreaterThan(0);
      expect(type.icon).toBeTruthy();
    }
  });

  it('customFieldTypeMeta は型ごとの日本語ラベルを返す', () => {
    expect(customFieldTypeMeta('text').label).toBe('テキスト');
    expect(customFieldTypeMeta('select').label).toBe('選択');
    expect(customFieldTypeMeta('checkbox').label).toBe('チェックボックス');
  });
});

describe('parseSelectOptions', () => {
  it('1行1つの入力を label/value ペアへ変換する', () => {
    expect(parseSelectOptions('高\n中\n低')).toEqual([
      { label: '高', value: '高' },
      { label: '中', value: '中' },
      { label: '低', value: '低' },
    ]);
  });

  it('空行と前後の空白を除去する（backend は空文字・非トリム値を拒否する）', () => {
    expect(parseSelectOptions('  高  \n\n 低 \n')).toEqual([
      { label: '高', value: '高' },
      { label: '低', value: '低' },
    ]);
  });

  it('重複 value を排除する（backend は重複を拒否する）', () => {
    expect(parseSelectOptions('高\n高\n低')).toEqual([
      { label: '高', value: '高' },
      { label: '低', value: '低' },
    ]);
  });

  it('空文字・空白のみの入力は空配列を返す', () => {
    expect(parseSelectOptions('')).toEqual([]);
    expect(parseSelectOptions(' \n \n')).toEqual([]);
  });
});

describe('serializeSelectOptions', () => {
  it('options 配列を 1 行に 1 つのテキストへ戻す（parseSelectOptions の逆）', () => {
    expect(
      serializeSelectOptions([
        { label: '高', value: '高' },
        { label: '中', value: '中' },
      ]),
    ).toBe('高\n中');
  });

  it('parseSelectOptions とラウンドトリップする', () => {
    const text = '高\n中\n低';
    expect(serializeSelectOptions(parseSelectOptions(text))).toBe(text);
  });

  it('配列でない値・value を持たない要素は安全に無視する', () => {
    expect(serializeSelectOptions(null)).toBe('');
    expect(serializeSelectOptions(undefined)).toBe('');
    expect(serializeSelectOptions('高')).toBe('');
    expect(serializeSelectOptions([{ label: '高' }, { value: '中' }])).toBe('中');
  });
});
