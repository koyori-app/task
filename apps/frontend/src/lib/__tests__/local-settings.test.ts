import { describe, expect, it } from 'vitest';
import {
  createSettingsSerializer,
  defaultValues,
  pickKnownValues,
  type SettingTable,
} from '@/lib/local-settings';

const TABLE = {
  alpha: { name: 'A', description: 'a', default: false },
  beta: { name: 'B', description: 'b', default: true },
} as const satisfies SettingTable;

describe('defaultValues', () => {
  it('表の既定値をそのまま返す', () => {
    expect(defaultValues(TABLE)).toEqual({ alpha: false, beta: true });
  });
});

describe('pickKnownValues', () => {
  it('表に在る key の真偽はそのまま残す', () => {
    expect(pickKnownValues(TABLE, { alpha: true, beta: false })).toEqual({
      alpha: true,
      beta: false,
    });
  });

  it('表に無い key は捨てる', () => {
    const picked = pickKnownValues(TABLE, { alpha: true, removed: true, __proto__: { x: 1 } });

    expect(Object.keys(picked).sort()).toEqual(['alpha', 'beta']);
    expect(picked).not.toHaveProperty('removed');
  });

  it('欠けた key と真偽でない値は既定値に戻す', () => {
    expect(pickKnownValues(TABLE, { alpha: 'yes', beta: 1 })).toEqual({ alpha: false, beta: true });
    expect(pickKnownValues(TABLE, {})).toEqual({ alpha: false, beta: true });
  });

  it.each([null, undefined, 'text', 3, true, []])('object でない入力 %j は全て既定値', (raw) => {
    expect(pickKnownValues(TABLE, raw)).toEqual({ alpha: false, beta: true });
  });

  it('prototype 上の名前（toString 等）は key として拾わぬ', () => {
    const table = { toString: { name: 't', description: 't', default: false } } as const;

    expect(pickKnownValues(table, {})).toEqual({ toString: false });
  });
});

describe('createSettingsSerializer', () => {
  const serializer = createSettingsSerializer(TABLE);

  it('書いた物を読み戻せる', () => {
    const text = serializer.serialize({ values: { alpha: true, beta: false } });

    expect(serializer.deserialize(text)).toEqual({ values: { alpha: true, beta: false } });
  });

  it('読み込む時に、表から消えた key を捨てる', () => {
    const text = JSON.stringify({ values: { alpha: true, gone: true } });

    expect(serializer.deserialize(text)).toEqual({ values: { alpha: true, beta: true } });
  });

  it.each(['{', 'null', '[]', '{"values":null}', '{"values":"x"}', '{}'])(
    '壊れた入力 %s は全て既定値に倒す',
    (text) => {
      expect(serializer.deserialize(text)).toEqual({ values: { alpha: false, beta: true } });
    },
  );
});
