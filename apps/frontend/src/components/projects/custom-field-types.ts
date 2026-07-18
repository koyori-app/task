import type { Component } from 'vue';
import {
  PhCalendarBlank,
  PhCheckSquare,
  PhHash,
  PhLink,
  PhList,
  PhTextAa,
} from '@phosphor-icons/vue';
import type { components } from '@/generated/api';

export type CustomFieldType = components['schemas']['CustomFieldType'];

export type CustomFieldSelectOption = { label: string; value: string };

type FieldTypeMeta = { label: string; icon: Component };

/**
 * 型ごとのメタ（デザインのアイコン対応: text-aa / hash / list / calendar-blank / link / check-square）。
 * `satisfies Record<CustomFieldType, …>` により、OpenAPI の CustomFieldType に
 * enum が増減すると型エラーになる（キーの過不足をコンパイル時に検出）。
 */
const CUSTOM_FIELD_TYPE_META = {
  text: { label: 'テキスト', icon: PhTextAa },
  number: { label: '数値', icon: PhHash },
  select: { label: '選択', icon: PhList },
  date: { label: '日付', icon: PhCalendarBlank },
  url: { label: 'URL', icon: PhLink },
  checkbox: { label: 'チェックボックス', icon: PhCheckSquare },
} satisfies Record<CustomFieldType, FieldTypeMeta>;

export const CUSTOM_FIELD_TYPES: { value: CustomFieldType; label: string; icon: Component }[] = (
  Object.keys(CUSTOM_FIELD_TYPE_META) as CustomFieldType[]
).map((value) => ({ value, ...CUSTOM_FIELD_TYPE_META[value] }));

export function customFieldTypeMeta(fieldType: CustomFieldType): FieldTypeMeta {
  return CUSTOM_FIELD_TYPE_META[fieldType];
}

/**
 * 「1行に1つ」形式の入力を select 型の options（label/value ペア）へ変換する。
 * backend は空配列・空文字・重複 value を拒否するため、ここで空行除去と重複排除を行う。
 */
export function parseSelectOptions(text: string): CustomFieldSelectOption[] {
  const seen = new Set<string>();
  const options: CustomFieldSelectOption[] = [];
  for (const line of text.split('\n')) {
    const value = line.trim();
    if (!value || seen.has(value)) continue;
    seen.add(value);
    options.push({ label: value, value });
  }
  return options;
}
