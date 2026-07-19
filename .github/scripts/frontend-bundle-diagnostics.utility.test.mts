/*
 * SPDX-FileCopyrightText: syuilo and misskey-project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { describe, expect, it } from 'vitest';
import {
  escapeTableCell,
  formatBytes,
  formatDelta,
  formatPercent,
} from './frontend-bundle-diagnostics.utility.mts';

describe('frontend-bundle-diagnostics.utility', () => {
  it('formats bytes across B/KB/MB boundaries', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(1500)).toBe('1.5 KB');
    expect(formatBytes(2_500_000)).toBe('2.50 MB');
    expect(formatBytes(-1500)).toBe('-1.5 KB');
  });

  it('formats signed deltas', () => {
    expect(formatDelta(1024)).toBe('+1.0 KB');
    expect(formatDelta(-512)).toBe('-512 B');
    expect(formatDelta(0)).toBe('0 B');
  });

  it('formats percent with before===0 branch', () => {
    expect(formatPercent(0, 0)).toBe('0.0%');
    expect(formatPercent(0, 100)).toBe('new');
    expect(formatPercent(100, 150)).toBe('+50.0%');
    expect(formatPercent(200, 100)).toBe('-50.0%');
  });

  it('escapes table cell pipes', () => {
    expect(escapeTableCell('a|b')).toBe('a\\|b');
  });
});
