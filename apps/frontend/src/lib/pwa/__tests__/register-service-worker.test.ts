import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  registerServiceWorker,
  resetServiceWorkerRegistrationForTest,
} from '../register-service-worker';

afterEach(() => resetServiceWorkerRegistrationForTest());

function fakeNavigator(register = vi.fn(() => Promise.resolve({} as ServiceWorkerRegistration))) {
  return { navigator: { serviceWorker: { register } } as unknown as Navigator, register };
}

describe('registerServiceWorker', () => {
  it('registers /sw.js with scope / only once', async () => {
    const { navigator, register } = fakeNavigator();

    const first = registerServiceWorker({ navigator, dev: false });
    const second = registerServiceWorker({ navigator, dev: false });

    expect(register).toHaveBeenCalledTimes(1);
    expect(register).toHaveBeenCalledWith('/sw.js', { scope: '/' });
    expect(second).toBe(first);
    await first;
  });

  it('does nothing on the server (no navigator)', () => {
    expect(registerServiceWorker({ navigator: undefined, dev: false })).toBeUndefined();
  });

  it('does nothing when the browser has no service worker API', () => {
    expect(registerServiceWorker({ navigator: {} as Navigator, dev: false })).toBeUndefined();
  });

  it('does not register under vike dev', () => {
    const { navigator, register } = fakeNavigator();

    expect(registerServiceWorker({ navigator, dev: true })).toBeUndefined();
    expect(register).not.toHaveBeenCalled();
  });

  it('does not reject on failure, and tries again on the next call', async () => {
    const register = vi
      .fn()
      .mockRejectedValueOnce(new Error('blocked'))
      .mockResolvedValueOnce({} as ServiceWorkerRegistration);
    const { navigator } = fakeNavigator(register);

    await expect(registerServiceWorker({ navigator, dev: false })).resolves.toBeUndefined();
    await expect(registerServiceWorker({ navigator, dev: false })).resolves.toEqual({});

    expect(register).toHaveBeenCalledTimes(2);
  });
});
