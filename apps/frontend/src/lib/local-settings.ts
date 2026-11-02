/**
 * この端末のこのブラウザだけで効く設定（環境設定・実験的機能）の表の型と、読み込みの道具。
 * 項目の定義（key・名前・説明・既定値）は表で一か所に持つ。localStorage に残った値は、
 * 読み込む時に表と突き合わせ、表に無い key は捨て、型に合わない値と欠けた key は既定値に戻す。
 * 項目は二種類ある。切り替え（真偽）と、選び肢から一つを選ぶ物（choices を持つ）。
 */
export type ToggleSettingDefinition = {
  name: string;
  description: string;
  default: boolean;
};

/** 選び肢から一つを選ぶ項目。値は choices の value のどれか。 */
export type ChoiceSettingDefinition = {
  name: string;
  description: string;
  choices: readonly { value: string; label: string }[];
  default: string;
};

export type SettingDefinition = ToggleSettingDefinition | ChoiceSettingDefinition;

export type SettingTable = Record<string, SettingDefinition>;

/** 項目の値の型。選び肢の項目は choices の value の和、切り替えは真偽。 */
export type SettingValue<D extends SettingDefinition> = D extends {
  choices: readonly { value: infer V }[];
}
  ? V
  : boolean;

export type SettingValues<T extends SettingTable> = {
  -readonly [K in keyof T]: SettingValue<T[K]>;
};

export function isChoiceSetting(
  definition: SettingDefinition,
): definition is ChoiceSettingDefinition {
  return 'choices' in definition;
}

/** 値がその項目の型に合うか。選び肢の項目は choices に在る value だけを認める。 */
function isValidValue(definition: SettingDefinition, value: unknown): boolean {
  if (isChoiceSetting(definition)) {
    return typeof value === 'string' && definition.choices.some((choice) => choice.value === value);
  }
  return typeof value === 'boolean';
}

export function defaultValues<T extends SettingTable>(table: T): SettingValues<T> {
  const values: Record<string, boolean | string> = {};
  for (const key of Object.keys(table)) values[key] = table[key]!.default;
  return values as SettingValues<T>;
}

/** 表に在る key だけを残し、値が項目の型に合わなければ既定値にする。入力が object でなければ全て既定値。 */
export function pickKnownValues<T extends SettingTable>(table: T, raw: unknown): SettingValues<T> {
  const source =
    typeof raw === 'object' && raw !== null && !Array.isArray(raw)
      ? (raw as Record<string, unknown>)
      : {};
  const values: Record<string, boolean | string> = {};
  for (const key of Object.keys(table)) {
    const definition = table[key]!;
    const value = Object.hasOwn(source, key) ? source[key] : undefined;
    values[key] = isValidValue(definition, value)
      ? (value as boolean | string)
      : definition.default;
  }
  return values as SettingValues<T>;
}

/**
 * pinia-plugin-persistedstate の serializer。store の `values` だけを JSON で残し、
 * 読み込む時に表と突き合わせる（壊れた JSON も既定値に倒す）。
 */
export function createSettingsSerializer<T extends SettingTable>(table: T) {
  return {
    serialize: (state: unknown) => JSON.stringify(state),
    deserialize: (text: string) => {
      let parsed: unknown;
      try {
        parsed = JSON.parse(text);
      } catch {
        parsed = null;
      }
      const raw =
        typeof parsed === 'object' && parsed !== null
          ? (parsed as Record<string, unknown>).values
          : undefined;
      return { values: pickKnownValues(table, raw) };
    },
  };
}
