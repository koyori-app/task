import { describe, expect, it } from 'vitest';
import { breadcrumbRoute, breadcrumbSegments, toBreadcrumbList } from '../breadcrumbs';

describe('パンくずのルート台帳', () => {
  it.each([
    ['/', 'ホーム'],
    ['/settings/profile', 'プロフィール'],
    ['/settings/security', 'セキュリティ'],
    ['/settings/tokens', 'API トークン'],
    ['/acme/my-tasks', 'ホーム'],
    ['/acme/settings', 'テナント設定'],
    ['/acme/settings/members', 'メンバー'],
    ['/acme/projects/new', 'プロジェクト作成'],
    ['/acme/projects/ENG/tasks', ''],
    ['/acme/projects/ENG/settings', 'プロジェクト設定'],
    ['/acme/projects/ENG/reviews', 'レビュー'],
    ['/acme/projects/ENG/labels', 'ラベル'],
    ['/acme/projects/ENG/tasks/ENG-3', ''],
  ])('%s はページと実データから段を組む', (pathname, name) => {
    expect(
      breadcrumbRoute(pathname, { tenant: 'acme', projectKey: 'ENG', taskId: 'ENG-3' })?.name,
    ).toBe(name);
  });

  it.each([
    '/forgotten',
    '/acme/unknown',
    '/signin',
    '/signup',
    '/verify-email',
    '/auth/reset-password',
  ])('%s は段を出さない', (pathname) => {
    expect(breadcrumbRoute(pathname, { tenant: 'acme' })).toBeUndefined();
  });
});

describe('BreadcrumbList への一方向の写像', () => {
  it('現在地にも内部URLを保持し、写像だけが末尾の item を省く', () => {
    const segments = breadcrumbSegments([
      { name: 'ホーム', href: '/acme/my-tasks' },
      { name: 'プロジェクト', href: '/acme/projects/ENG/tasks' },
    ]);
    const before = structuredClone(segments);
    expect(toBreadcrumbList(segments, 'https://task.example/nested/path')).toEqual({
      '@context': 'https://schema.org',
      '@type': 'BreadcrumbList',
      itemListElement: [
        {
          '@type': 'ListItem',
          position: 1,
          name: 'ホーム',
          item: 'https://task.example/acme/my-tasks',
        },
        { '@type': 'ListItem', position: 2, name: 'プロジェクト' },
      ],
    });
    expect(segments).toEqual(before);
    expect(segments.at(-1)).toEqual({
      name: 'プロジェクト',
      href: '/acme/projects/ENG/tasks',
      current: true,
    });
  });

  it('空の段とHTTP以外の origin を扱う', () => {
    expect(toBreadcrumbList([], 'https://task.example').itemListElement).toEqual([]);
    expect(() => toBreadcrumbList([], 'file:///tmp')).toThrow(TypeError);
  });
});
