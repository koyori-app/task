import { flushPromises } from '@vue/test-utils';
import { nextTick, ref } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { usePasswordStrength, type PasswordStrength } from '../usePasswordStrength';

function mockFetch(strength: PasswordStrength, ok = true) {
  return vi.fn().mockResolvedValue(
    new Response(JSON.stringify({ strength }), {
      status: ok ? 200 : 500,
      headers: { 'Content-Type': 'application/json' },
    }),
  );
}

describe('usePasswordStrength', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('初期値は空文字', () => {
    const password = ref('');
    const { strength } = usePasswordStrength(password);
    expect(strength.value).toBe('');
  });

  it('空文字のままでは fetch しない', async () => {
    const fetchMock = mockFetch('low');
    vi.stubGlobal('fetch', fetchMock);

    const password = ref('');
    usePasswordStrength(password);

    await vi.advanceTimersByTimeAsync(300);
    await flushPromises();

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('300ms 後に POST /internal/password-strength を呼ぶ', async () => {
    const fetchMock = mockFetch('low');
    vi.stubGlobal('fetch', fetchMock);

    const password = ref('');
    usePasswordStrength(password);

    password.value = 'hello123';
    await nextTick();

    await vi.advanceTimersByTimeAsync(299);
    expect(fetchMock).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    await flushPromises();

    expect(fetchMock).toHaveBeenCalledOnce();
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body as string)).toEqual({ password: 'hello123' });
  });

  it.each([
    ['low', '12345678'],
    ['low', 'Password1'],
    ['low', 'P@ssw0rd'],
    ['medium', 'moderatePass99'],
    ['high', 'Tr0ub4dor&3'],
  ] as [PasswordStrength, string][])(
    'API が %s を返したとき strength が %s になる',
    async (expectedStrength, pw) => {
      vi.stubGlobal('fetch', mockFetch(expectedStrength));

      const password = ref('');
      const { strength } = usePasswordStrength(password);

      password.value = pw;
      await nextTick();
      await vi.advanceTimersByTimeAsync(300);
      await flushPromises();

      expect(strength.value).toBe(expectedStrength);
    },
  );

  it('パスワードを空にしたとき即座に "" に戻る', async () => {
    vi.stubGlobal('fetch', mockFetch('high'));

    const password = ref('');
    const { strength } = usePasswordStrength(password);

    password.value = 'Tr0ub4dor&3';
    await nextTick();
    await vi.advanceTimersByTimeAsync(300);
    await flushPromises();
    expect(strength.value).toBe('high');

    password.value = '';
    await nextTick();
    await flushPromises();
    expect(strength.value).toBe('');
  });

  it('API がエラーを返したとき strength を更新しない', async () => {
    vi.stubGlobal('fetch', mockFetch('low', false));

    const password = ref('hello123');
    const { strength } = usePasswordStrength(password);

    await vi.advanceTimersByTimeAsync(300);
    await flushPromises();

    expect(strength.value).toBe('');
  });

  it('fetch がネットワークエラーで reject したとき strength を更新しない', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('network error')));

    const password = ref('hello123');
    const { strength } = usePasswordStrength(password);

    await vi.advanceTimersByTimeAsync(300);
    await flushPromises();

    expect(strength.value).toBe('');
  });

  it('レスポンスが JSON でないとき（nginx 等）strength を更新しない', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('<html>Welcome to nginx!</html>', {
          status: 200,
          headers: { 'Content-Type': 'text/html' },
        }),
      ),
    );

    const password = ref('hello123');
    const { strength } = usePasswordStrength(password);

    await vi.advanceTimersByTimeAsync(300);
    await flushPromises();

    expect(strength.value).toBe('');
  });

  it('高速入力時に古いレスポンスを破棄する', async () => {
    let resolveFirst!: (v: Response) => void;
    const firstResponse = new Promise<Response>(
      (res) => (resolveFirst = res),
    );

    const fetchMock = vi
      .fn()
      .mockReturnValueOnce(firstResponse)
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ strength: 'high' }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    vi.stubGlobal('fetch', fetchMock);

    const password = ref('');
    const { strength } = usePasswordStrength(password);

    password.value = 'short';
    await nextTick();
    await vi.advanceTimersByTimeAsync(300);

    password.value = 'Tr0ub4dor&3';
    await nextTick();
    await vi.advanceTimersByTimeAsync(300);
    await flushPromises();

    // 2件目のレスポンスで high になる
    expect(strength.value).toBe('high');

    // 1件目（stale）を遅れて resolve しても上書きされない
    resolveFirst(
      new Response(JSON.stringify({ strength: 'low' }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
    await flushPromises();

    expect(strength.value).toBe('high');
  });
});
