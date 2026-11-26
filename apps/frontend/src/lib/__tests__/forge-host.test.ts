import { describe, expect, it } from 'vitest';

import { commitHref, forgeHost, pullRequestHref, type ForgeTarget } from '@/lib/forge-host';

const SHA = 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678';
const github: ForgeTarget = { host: 'github', html_url: null, repository: 'koyori-app/task' };
const gitlab: ForgeTarget = { host: 'gitlab', html_url: null, repository: 'koyori-app/task' };

describe('forge host', () => {
  it('github と gitlab を見分け、知らない値と null は null', () => {
    expect(forgeHost('github')).toMatchObject({ kind: 'github', label: 'GitHub' });
    expect(forgeHost('gitlab')).toMatchObject({ kind: 'gitlab', label: 'GitLab' });
    expect(forgeHost('forgejo')).toBeNull();
    expect(forgeHost(null)).toBeNull();
    expect(forgeHost(undefined)).toBeNull();
  });

  it('PR の URL をホストごとの形で組む', () => {
    expect(pullRequestHref(github, 231)).toBe('https://github.com/koyori-app/task/pull/231');
    expect(pullRequestHref(gitlab, 231)).toBe(
      'https://gitlab.com/koyori-app/task/-/merge_requests/231',
    );
  });

  it('コミットの URL をホストごとの形で組む', () => {
    expect(commitHref(github, SHA)).toBe(`https://github.com/koyori-app/task/commit/${SHA}`);
    expect(commitHref(gitlab, SHA)).toBe(`https://gitlab.com/koyori-app/task/-/commit/${SHA}`);
  });

  it('html_url があれば PR の URL にそれを使い、コミットの URL はその origin から組む', () => {
    const target: ForgeTarget = {
      host: 'github',
      html_url: 'https://ghe.example.com/koyori-app/task/pull/231',
      repository: 'koyori-app/task',
    };
    expect(pullRequestHref(target, 231)).toBe('https://ghe.example.com/koyori-app/task/pull/231');
    expect(commitHref(target, SHA)).toBe(`https://ghe.example.com/koyori-app/task/commit/${SHA}`);
  });

  it('html_url が http(s) でなければ使わない', () => {
    const target: ForgeTarget = { ...github, html_url: 'javascript:alert(1)' };
    expect(pullRequestHref(target, 231)).toBe('https://github.com/koyori-app/task/pull/231');
    expect(commitHref(target, SHA)).toBe(`https://github.com/koyori-app/task/commit/${SHA}`);
  });

  it('知らないホスト・リポジトリが無い・コミットの形でないものは null', () => {
    expect(pullRequestHref({ ...github, host: 'forgejo' }, 231)).toBeNull();
    expect(pullRequestHref({ ...github, host: null }, 231)).toBeNull();
    expect(pullRequestHref({ ...github, repository: null }, 231)).toBeNull();
    expect(commitHref({ ...github, repository: null }, SHA)).toBeNull();
    expect(commitHref({ ...github, host: 'forgejo' }, SHA)).toBeNull();
    expect(commitHref(github, '../../evil')).toBeNull();
  });
});
