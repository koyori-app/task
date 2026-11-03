import type { APIRequestContext } from '@playwright/test';

const API_URL = process.env.API_URL ?? 'http://localhost:3400';

export type MentionFixture = {
  tenantId: string;
  tenantDisplayId: string;
  projectId: string;
  projectKey: string;
  taskSeqId: number;
  taskHref: string;
};

/** 認証済み request で個人プロジェクトに課題を1件用意する（メンション候補用のメンバーは e2e ユーザー自身のみでも @ 起動は可能）。 */
export async function ensureMentionTask(request: APIRequestContext): Promise<MentionFixture> {
  const tenantsRes = await request.get(`${API_URL}/v1/tenants`);
  if (!tenantsRes.ok()) {
    throw new Error(`GET /v1/tenants failed: ${tenantsRes.status()} ${await tenantsRes.text()}`);
  }
  const tenantsBody = (await tenantsRes.json()) as { items?: { id: string; display_id: string }[] };
  let tenant = tenantsBody.items?.[0];
  if (!tenant) {
    const suffix = Date.now().toString(36);
    const createRes = await request.post(`${API_URL}/v1/tenants`, {
      data: { name: `e2e-${suffix}`, display_id: `e2e${suffix}`.slice(0, 32) },
    });
    if (!createRes.ok()) {
      throw new Error(`POST /v1/tenants failed: ${createRes.status()} ${await createRes.text()}`);
    }
    tenant = (await createRes.json()) as { id: string; display_id: string };
  }

  const projectRes = await request.get(
    `${API_URL}/v1/tenants/${tenant.id}/users/me/personal-project`,
  );
  if (!projectRes.ok()) {
    throw new Error(
      `GET personal-project failed: ${projectRes.status()} ${await projectRes.text()}`,
    );
  }
  const project = (await projectRes.json()) as { id: string; key: string };

  const taskRes = await request.post(
    `${API_URL}/v1/tenants/${tenant.id}/projects/${project.id}/tasks`,
    { data: { title: `mention-tooltip-${Date.now()}`, description: '' } },
  );
  if (!taskRes.ok()) {
    throw new Error(`POST task failed: ${taskRes.status()} ${await taskRes.text()}`);
  }
  const task = (await taskRes.json()) as { seq_id: number };

  const taskHref = `/${tenant.display_id}/projects/${project.key}/tasks/${project.key}-${task.seq_id}`;

  return {
    tenantId: tenant.id,
    tenantDisplayId: tenant.display_id,
    projectId: project.id,
    projectKey: project.key,
    taskSeqId: task.seq_id,
    taskHref,
  };
}
