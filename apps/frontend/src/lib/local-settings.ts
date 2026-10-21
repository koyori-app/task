/**
 * この端末のこのブラウザだけで効く設定（環境設定・実験的機能）の表の型と、読み込みの道具。
 * 項目の定義（key・名前・説明・既定値）は表で一か所に持つ。localStorage に残った値は、
 * 読み込む時に表と突き合わせ、表に無い key は捨て、真偽でない値と欠けた key は既定値に戻す。
 */
export type SettingDefinition = {
  name: string;
  description: string;
  default: boolean;
};

export type SettingTable = Record<string, SettingDefinition>;

export type SettingValues<T extends SettingTable> = { -readonly [K in keyof T]: boolean };

export function defaultValues<T extends SettingTable>(table: T): SettingValues<T> {
  const values: Record<string, boolean> = {};
  for (const key of Object.keys(table)) values[key] = table[key]!.default;
  return values as SettingValues<T>;
}

/** 表に在る key だけを残し、値が真偽でなければ既定値にする。入力が object でなければ全て既定値。 */
export function pickKnownValues<T extends SettingTable>(table: T, raw: unknown): SettingValues<T> {
  const source =
    typeof raw === 'object' && raw !== null && !Array.isArray(raw)
      ? (raw as Record<string, unknown>)
      : {};
  const values: Record<string, boolean> = {};
  for (const key of Object.keys(table)) {
    const value = Object.hasOwn(source, key) ? source[key] : undefined;
    values[key] = typeof value === 'boolean' ? value : table[key]!.default;
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
