import { describe, it, expect } from 'vitest';
import { safeRedirectPath, signInUrl, withRedirectAfter } from '../useRedirectAfter';

describe('safeRedirectPath', () => {
  it('同一オリジンの相対パスは query・hash ごと通す', () => {
    expect(safeRedirectPath('/t/x/projects/P/tasks/P-1?foo=1#c')).toBe(
      '/t/x/projects/P/tasks/P-1?foo=1#c',
    );
  });

  it.each([
    'https://evil.example',
    '//evil.example',
    '/\\evil.example',
    'javascript:alert(1)',
    'evil',
    '/\t/evil.example',
    '',
    null,
    undefined,
  ])('%s は / にする', (v) => {
    expect(safeRedirectPath(v as string | null | undefined)).toBe('/');
  });
});

describe('signInUrl / withRedirectAfter', () => {
  it('戻り先を redirect_after に載せ、/ のときは付けない', () => {
    expect(signInUrl('/t/x?foo=1')).toBe('/signin?redirect_after=%2Ft%2Fx%3Ffoo%3D1');
    expect(signInUrl('//evil.example')).toBe('/signin');
    expect(withRedirectAfter('/signup', '/')).toBe('/signup');
    expect(withRedirectAfter('/signup', '/a?b=1')).toBe('/signup?redirect_after=%2Fa%3Fb%3D1');
  });
});
