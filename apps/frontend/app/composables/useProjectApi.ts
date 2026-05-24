import { useRuntimeConfig } from '#app';

export type Project = {
  id: string;
  name: string;
  description: string;
  tenant_id: string;
};

export type CreateProjectPayload = {
  name: string;
  description?: string;
};

export type UpdateProjectPayload = {
  name?: string;
  description?: string;
};

export function useProjectApi(tenantId: string) {
  const runtimeConfig = useRuntimeConfig();
  const apiBase = String(runtimeConfig.public.apiBase ?? '/api').replace(/\/$/, '');
  const base = `${apiBase}/v1/tenants/${tenantId}/projects`;

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
    list: () => request<Project[]>(base),
    create: (payload: CreateProjectPayload) =>
      request<Project>(base, { method: 'POST', body: JSON.stringify(payload) }),
    get: (id: string) => request<Project>(`${base}/${id}`),
    update: (id: string, payload: UpdateProjectPayload) =>
      request<Project>(`${base}/${id}`, { method: 'PUT', body: JSON.stringify(payload) }),
    remove: (id: string) => request<void>(`${base}/${id}`, { method: 'DELETE' }),
  };
}
