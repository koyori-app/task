import type { PageContext } from 'vike/types';

export default (pageContext: PageContext) =>
  pageContext.urlPathname.replace(/\/$/, '') === `/${pageContext.routeParams.tenant}`
    ? 'ホーム | Koyori'
    : 'Koyori';
