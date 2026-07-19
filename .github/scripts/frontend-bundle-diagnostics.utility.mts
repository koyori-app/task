/*
 * SPDX-FileCopyrightText: syuilo and misskey-project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

export function formatBytes(value: number) {
  const sign = value < 0 ? '-' : '';
  const absolute = Math.abs(value);
  if (absolute < 1000) return `${sign}${absolute.toFixed(0)} B`;
  if (absolute < 1_000_000) return `${sign}${(absolute / 1000).toFixed(1)} KB`;
  return `${sign}${(absolute / 1_000_000).toFixed(2)} MB`;
}

export function formatDelta(value: number) {
  return `${value > 0 ? '+' : ''}${formatBytes(value)}`;
}

export function formatPercent(before: number, after: number) {
  if (before === 0) return after === 0 ? '0.0%' : 'new';
  const percent = ((after - before) / before) * 100;
  return `${percent > 0 ? '+' : ''}${percent.toFixed(1)}%`;
}

export function escapeTableCell(value: string) {
  return value
    .replaceAll(/[\r\n]+/g, ' ')
    .replaceAll('`', "'")
    .replaceAll('|', '\\|');
}
