import { describe, expect, it } from 'vitest';
import { requiresTwoFactor, safeRedirectAfter, twoFactorPath } from '../two-factor';

describe('safeRedirectAfter', () => {
  it.each([
    ['/', '/'],
    ['/foo', '/foo'],
    ['/foo/bar?x=1#h', '/foo/bar?x=1#h'],
    ['/desktop/authorize?port=1234', '/desktop/authorize?port=1234'],
  ])('同一オリジンの相対パス %s は通す', (input, expected) => {
    expect(safeRedirectAfter(input)).toBe(expected);
  });

  it.each([
    'https://evil.example/',
    'http://evil.example',
    '//evil.example',
    '//evil.example/path',
    '/\\evil.example',
    '\\\\evil.example',
    'javascript:alert(1)',
    'evil',
    '',
    '/foo\nbar',
    '/auth/2fa',
    '/auth/2fa?redirect_after=%2F',
  ])('%j は / にする', (input) => {
    expect(safeRedirectAfter(input)).toBe('/');
  });

  it('null / undefined は /', () => {
    expect(safeRedirectAfter(null)).toBe('/');
    expect(safeRedirectAfter(undefined)).toBe('/');
  });
});

describe('twoFactorPath', () => {
  it('戻り先が / なら付けない', () => {
    expect(twoFactorPath('/')).toBe('/auth/2fa');
    expect(twoFactorPath(null)).toBe('/auth/2fa');
  });

  it('戻り先は検証して載せる', () => {
    expect(twoFactorPath('/foo?a=1')).toBe('/auth/2fa?redirect_after=%2Ffoo%3Fa%3D1');
    expect(twoFactorPath('//evil.example')).toBe('/auth/2fa');
  });
});

describe('requiresTwoFactor', () => {
  it('requires_2fa / requires_2fa_setup のどちらかが true なら true', () => {
    expect(requiresTwoFactor({ requires_2fa: true, requires_2fa_setup: false })).toBe(true);
    expect(requiresTwoFactor({ requires_2fa: false, requires_2fa_setup: true })).toBe(true);
  });

  it('どちらも false、または本文なし（204）なら false', () => {
    expect(requiresTwoFactor({ requires_2fa: false, requires_2fa_setup: false })).toBe(false);
    expect(requiresTwoFactor(undefined)).toBe(false);
    expect(requiresTwoFactor({})).toBe(false);
  });
});
