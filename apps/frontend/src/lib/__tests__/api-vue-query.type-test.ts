import type { ProjectUuid, TenantUuid } from '../api-ids';
import { projectLabelsQueryOptions, projectsQueryOptions } from '../api-vue-query';

declare const tenantUuid: TenantUuid;
declare const projectUuid: ProjectUuid;

projectsQueryOptions(tenantUuid);
projectLabelsQueryOptions(tenantUuid, projectUuid);

// @ts-expect-error A tenant display ID is not a resolved tenant UUID.
projectsQueryOptions('tenant-display-id');

// @ts-expect-error A project key is not a resolved project UUID.
projectLabelsQueryOptions(tenantUuid, 'PROJECT_KEY');
