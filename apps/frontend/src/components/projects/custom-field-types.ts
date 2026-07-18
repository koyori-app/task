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

/** デザインのアイコン対応（text-aa / hash / list / calendar-blank / link / check-square）に合わせた型メタ */
export const CUSTOM_FIELD_TYPES: { value: CustomFieldType; label: string; icon: Component }[] = [
  { value: 'text', label: 'テキスト', icon: PhTextAa },
  { value: 'number', label: '数値', icon: PhHash },
  { value: 'select', label: '選択', icon: PhList },
  { value: 'date', label: '日付', icon: PhCalendarBlank },
  { value: 'url', label: 'URL', icon: PhLink },
  { value: 'checkbox', label: 'チェックボックス', icon: PhCheckSquare },
];

export function customFieldTypeMeta(fieldType: CustomFieldType) {
  return CUSTOM_FIELD_TYPES.find((t) => t.value === fieldType) ?? CUSTOM_FIELD_TYPES[0];
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
