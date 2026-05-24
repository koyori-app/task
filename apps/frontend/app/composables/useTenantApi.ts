import { useRuntimeConfig } from '#app';

export type Tenant = {
  id: string;
  display_id: string;
  name: string;
  description: string;
  icon_url: string;
  owner_id: string;
};

export type CreateTenantPayload = {
  display_id: string;
  name: string;
  description?: string;
  icon_url?: string;
};

export type UpdateTenantPayload = {
  name?: string;
  description?: string;
  icon_url?: string;
};

export function useTenantApi() {
  const runtimeConfig = useRuntimeConfig();
  const apiBase = String(runtimeConfig.public.apiBase ?? '/api').replace(/\/$/, '');
  const base = `${apiBase}/v1/tenants`;

  async function request<T>(url: string, init: RequestInit = {}): Promise<T> {
    const res = await fetch(url, {
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      credentials: 'include',
      ...init,
    });
    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw new Error(text || `Request failed: ${res.status}`);
    }
    if (res.status === 204) return undefined as T;
    return res.json();
  }

  return {
    list: () => request<Tenant[]>(base),
    create: (payload: CreateTenantPayload) =>
      request<Tenant>(base, { method: 'POST', body: JSON.stringify(payload) }),
    get: (id: string) => request<Tenant>(`${base}/${id}`),
    update: (id: string, payload: UpdateTenantPayload) =>
      request<Tenant>(`${base}/${id}`, { method: 'PUT', body: JSON.stringify(payload) }),
    remove: (id: string) => request<void>(`${base}/${id}`, { method: 'DELETE' }),
  };
}
