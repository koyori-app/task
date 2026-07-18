import { describe, it, expect } from 'vitest';
import { CUSTOM_FIELD_TYPES, customFieldTypeMeta, parseSelectOptions } from '../custom-field-types';

describe('CUSTOM_FIELD_TYPES', () => {
  it('openapi の CustomFieldType enum と同じ6種類を過不足なく提供する', () => {
    expect(CUSTOM_FIELD_TYPES.map((t) => t.value)).toEqual([
      'text',
      'number',
      'select',
      'date',
      'url',
      'checkbox',
    ]);
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
