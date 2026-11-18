import { describe, expect, it } from 'vitest';

import { ratio, token, type ThemeName } from '@/lib/__tests__/design-token';

import { GANTT_PALETTE } from '../chart';

/**
 * ガントの palette の当て方を、ui-vue の token の実の値（明るい配色と暗い配色）で測る。
 * 棒や線と、その下の地の間は WCAG の非文字の対比 3:1 以上、字は 4.5:1 以上を求める。
 * 範囲の外の印は細い帯の上に棒と並んで載るので、7:1 以上（AAA）を求める。
 */
type ColorName = keyof typeof GANTT_PALETTE;

function color(theme: ThemeName, name: ColorName): [number, number, number] {
  const value = GANTT_PALETTE[name];
  const m = value ? /^var\(--([\w-]+)\)$/.exec(value) : null;
  if (!m) throw new Error(`${name} が token の var() でない: ${value}`);
  return token(theme, m[1]!);
}

const contrast = (theme: ThemeName, a: ColorName, b: ColorName) =>
  ratio(color(theme, a), color(theme, b));

/** 色の濃さ（線形 sRGB の最大と最小の差）。白・黒・灰はほぼ 0 */
const chroma = (rgb: number[]) => Math.max(...rgb) - Math.min(...rgb);
/** 一番強い成分（0 赤・1 緑・2 青）。色相の大きな違いを見る */
const dominant = (rgb: number[]) => rgb.indexOf(Math.max(...rgb));

// 図の地は添えの字の帯（NoteBg）と同じ色
const GROUND: ColorName = 'NoteBg';

describe.each(['light', 'dark'] as const)('ガントの palette の対比（%s）', (theme) => {
  it.each([
    ['未着手の棒の地と図の地', 'BarBg', GROUND],
    ['未着手の棒の地と土日の帯', 'BarBg', 'Weekend'],
    ['進みと未着手の棒の地', 'TierLow', 'BarBg'],
    ['済みの棒の進みと済みの棒の地', 'TierDone', 'DoneBg'],
    ['済みの棒の縁と図の地', 'DoneStroke', GROUND],
    ['済みの棒の縁と土日の帯', 'DoneStroke', 'Weekend'],
    ['目安を過ぎた棒の縁と図の地', 'WarnStroke', GROUND],
    ['厳守の菱形と土日の帯', 'HardDeadline', 'Weekend'],
    ['今日の線と図の地', 'Today', GROUND],
    ['今日の線と土日の帯', 'Today', 'Weekend'],
    ['稲妻線と図の地', 'ProgressLine', GROUND],
    ['依存の矢印と図の地', 'Dep', GROUND],
    ['依存の矢印と土日の帯', 'Dep', 'Weekend'],
    ['待ちで遅れの矢印と図の地', 'DepDelayed', GROUND],
    ['待ちで遅れの矢印と土日の帯', 'DepDelayed', 'Weekend'],
    ['済みの矢印と図の地', 'DepDone', GROUND],
    ['済みの矢印と土日の帯', 'DepDone', 'Weekend'],
    ['済みの矢印と群れの帯', 'DepDone', 'GroupRow'],
  ] as const)('%s は 3:1 以上', (_, a, b) => {
    expect(contrast(theme, a, b)).toBeGreaterThanOrEqual(3);
  });

  it.each([
    ['日の数字と見出しの帯', 'DayNumber', 'HeaderBg', 4.5],
    ['曜日と見出しの帯', 'GridLabel', 'HeaderBg', 4.5],
    ['範囲の外の印と添えの字の帯', 'Note', 'NoteBg', 7],
  ] as const)('%s は目安（字は 4.5:1、範囲の外の印は 7:1）以上', (_, a, b, min) => {
    expect(contrast(theme, a, b)).toBeGreaterThanOrEqual(min);
  });

  it('群れの帯と土日の帯は別の段で、明度が一段（1.1 倍）以上離れる', () => {
    expect(GANTT_PALETTE.GroupRow).not.toBe(GANTT_PALETTE.Weekend);
    expect(contrast(theme, 'GroupRow', 'Weekend')).toBeGreaterThanOrEqual(1.1);
  });

  it('今日の線と稲妻線は色相で分かれる（どちらも灰でなく、強い成分が違う）', () => {
    const today = color(theme, 'Today');
    const line = color(theme, 'ProgressLine');
    expect(chroma(today)).toBeGreaterThan(0.1);
    expect(chroma(line)).toBeGreaterThan(0.1);
    expect(dominant(today)).not.toBe(dominant(line));
  });
});
