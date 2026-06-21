import { describe, expect, it } from 'vitest';
import type { RuntimeAdapter } from '@universal-middleware/core';

import { prehydrationQueryGuard } from '../prehydration-query-guard';

const runtime = { runtime: 'node', adapter: 'other', params: undefined } satisfies RuntimeAdapter;

async function runGuard(url: string, method = 'GET') {
  return prehydrationQueryGuard(new Request(url, { method }), {}, runtime);
}

describe('prehydrationQueryGuard', () => {
  it('redirects guarded paths case-insensitively while preserving the original path casing', async () => {
    const response = await runGuard(
      'https://example.test/SignIn?email=a%40b.test&next=/home#section',
    );

    expect(response).toBeInstanceOf(Response);
    expect((response as Response).status).toBe(302);
    expect((response as Response).headers.get('Location')).toBe('/SignIn?next=%2Fhome#section');
  });

  it('does not redirect unguarded paths with sensitive query keys', async () => {
    const response = await runGuard('https://example.test/profile?email=a%40b.test');

    expect(response).not.toBeInstanceOf(Response);
  });

  it('does not redirect non-GET requests', async () => {
    const response = await runGuard('https://example.test/signin?email=a%40b.test', 'POST');

    expect(response).not.toBeInstanceOf(Response);
  });
});
