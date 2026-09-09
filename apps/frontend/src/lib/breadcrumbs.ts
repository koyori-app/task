export type BreadcrumbSegment = {
  name: string;
  href: string;
  current: boolean;
};

export type BreadcrumbState = {
  segments: BreadcrumbSegment[];
  loading: boolean;
};

export function breadcrumbSegments(
  entries: readonly { name: string; href: string }[],
): BreadcrumbSegment[] {
  return entries.map((entry, index) => ({ ...entry, current: index === entries.length - 1 }));
}

/** 将来の公開ページ用。画面には差し込まず、呼び出し側が公開可否と origin を決める。 */
export function toBreadcrumbList(segments: readonly BreadcrumbSegment[], origin: string) {
  const base = new URL(origin);
  if (!['https:', 'http:'].includes(base.protocol)) {
    throw new TypeError('Breadcrumb origin must use HTTP or HTTPS');
  }
  return {
    '@context': 'https://schema.org',
    '@type': 'BreadcrumbList',
    itemListElement: segments.map((segment, index) => ({
      '@type': 'ListItem',
      position: index + 1,
      name: segment.name,
      ...(index === segments.length - 1 ? {} : { item: new URL(segment.href, base.origin).href }),
    })),
  };
}

/** 共通レイアウトのルート台帳。未知のページに他ページの段を流用しない。 */
export function breadcrumbRoute(pathname: string, params: Record<string, string | undefined>) {
  const tenant = encodeURIComponent(params.tenant ?? '');
  const project = encodeURIComponent(params.projectKey ?? '');
  const task = encodeURIComponent(params.taskId ?? '');
  const tenantBase = `/${tenant}`;
  const projectBase = `${tenantBase}/projects/${project}`;
  const routes = new Map([
    ['/', { kind: 'static', name: 'ホーム' }],
    ['/settings/profile', { kind: 'static', name: 'プロフィール' }],
    ['/settings/security', { kind: 'static', name: 'セキュリティ' }],
    ['/settings/tokens', { kind: 'static', name: 'API トークン' }],
    ...(tenant
      ? ([
          [`${tenantBase}/my-tasks`, { kind: 'home', name: 'ホーム' }],
          [`${tenantBase}/settings`, { kind: 'tenant', name: 'テナント設定' }],
          [`${tenantBase}/settings/members`, { kind: 'tenant', name: 'メンバー' }],
          [`${tenantBase}/projects/new`, { kind: 'tenant', name: 'プロジェクト作成' }],
        ] as const)
      : []),
    ...(tenant && project
      ? ([
          [`${projectBase}/tasks`, { kind: 'project', name: '' }],
          [`${projectBase}/settings`, { kind: 'project', name: 'プロジェクト設定' }],
          [`${projectBase}/reviews`, { kind: 'project', name: 'レビュー' }],
          [`${projectBase}/labels`, { kind: 'project', name: 'ラベル' }],
          ...(task
            ? ([[`${projectBase}/tasks/${task}`, { kind: 'task', name: '' }]] as const)
            : []),
        ] as const)
      : []),
  ]);
  return routes.get(pathname.replace(/\/$/, '') || '/');
}
