import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import createFetchClient from 'openapi-fetch';
import type { paths } from '@/generated/api';
import {
  meRetry,
  sessionExpired,
  sessionExpiredMiddleware,
  SESSION_EXPIRED_REDIRECT_DELAY_MS,
} from '../api-vue-query';

function clientReturning(status: number, message: string) {
  const client = createFetchClient<paths>({
    baseUrl: 'http://test.local/api',
    fetch: async () =>
      new Response(JSON.stringify({ message }), {
        status,
        headers: { 'Content-Type': 'application/json' },
      }),
  });
  client.use(sessionExpiredMiddleware);
  return client;
}

describe('sessionExpiredMiddleware', () => {
  let assignSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.useFakeTimers();
    sessionExpired.value = false;
    assignSpy = vi.spyOn(window.location, 'assign').mockImplementation(() => {});
  });

  afterEach(() => {
    vi.useRealTimers();
    assignSpy.mockRestore();
  });

  it('shows the notice, then sends the user to /signin on an in-session 401', async () => {
    await clientReturning(401, 'unauthorized').GET('/v1/tenants');
    expect(sessionExpired.value).toBe(true);
    expect(assignSpy).not.toHaveBeenCalled();
    vi.advanceTimersByTime(SESSION_EXPIRED_REDIRECT_DELAY_MS);
    expect(assignSpy).toHaveBeenCalledWith('/signin');
  });

  it.each(['invalid-credentials', 'invalid-2fa-code'])('ignores sign-in failure %s', async (m) => {
    await clientReturning(401, m).GET('/v1/tenants');
    expect(sessionExpired.value).toBe(false);
  });

  it('ignores non-401 responses', async () => {
    await clientReturning(500, 'unauthorized').GET('/v1/tenants');
    expect(sessionExpired.value).toBe(false);
  });

  it('leaves /v1/auth/me to useAuthSession', async () => {
    await clientReturning(401, 'unauthorized').GET('/v1/auth/me');
    expect(sessionExpired.value).toBe(false);
  });
});

describe('meRetry', () => {
  it('retries 5xx and network errors twice, never 401', () => {
    expect(meRetry(0, { response: { status: 500 } })).toBe(true);
    expect(meRetry(1, new TypeError('Failed to fetch'))).toBe(true);
    expect(meRetry(2, { response: { status: 500 } })).toBe(false);
    expect(meRetry(0, { response: { status: 401 } })).toBe(false);
  });
});
