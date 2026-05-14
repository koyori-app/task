import { useRuntimeConfig } from '#app';

type LoginPayload = {
  email: string;
  password: string;
};

type RegisterPayload = LoginPayload & {
  username: string;
};

export type CurrentUser = {
  id: string;
  username: string;
  bio: string | null;
  avatar_url: string | null;
  email: string;
  password_hash?: string | null;
};

async function readResponseMessage(response: Response): Promise<string> {
  const contentType = response.headers.get('content-type') ?? '';

  if (contentType.includes('application/json')) {
    const data = await response.json().catch(() => null);

    if (typeof data === 'string') {
      return data;
    }

    if (data && typeof data === 'object' && 'message' in data && typeof data.message === 'string') {
      return data.message;
    }

    return data ? JSON.stringify(data) : '';
  }

  return response.text();
}

export function useAuthApi() {
  const runtimeConfig = useRuntimeConfig();
  const apiBase = String(runtimeConfig.public.apiBase ?? '/api').replace(/\/$/, '');

  async function postAuth(path: string, payload: LoginPayload | RegisterPayload): Promise<string> {
    const response = await fetch(`${apiBase}${path}`, {
      method: 'POST',
      headers: {
        Accept: 'text/plain, application/json',
        'Content-Type': 'application/json',
      },
      credentials: 'include',
      body: JSON.stringify(payload),
    });

    const message = await readResponseMessage(response);

    if (!response.ok) {
      throw new Error(message || `Request failed with status ${response.status}`);
    }

    return message;
  }

  async function getCurrentUser(): Promise<CurrentUser> {
    const response = await fetch(`${apiBase}/v1/auth/me`, {
      method: 'GET',
      headers: {
        Accept: 'application/json, text/plain',
      },
      credentials: 'include',
    });

    if (!response.ok) {
      const message = await readResponseMessage(response);
      throw new Error(message || `Request failed with status ${response.status}`);
    }

    return response.json();
  }

  return {
    login: (payload: LoginPayload) => postAuth('/v1/auth/login', payload),
    register: (payload: RegisterPayload) => postAuth('/v1/auth/register', payload),
    getCurrentUser,
  };
}
