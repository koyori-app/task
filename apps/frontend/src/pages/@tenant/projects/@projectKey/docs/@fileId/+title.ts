import type { PageContext } from 'vike/types';

import type { Data } from './+data';

export default (pageContext: PageContext<Data>) =>
  pageContext.data?.title ? `${pageContext.data.title} | Koyori` : 'ドキュメント | Koyori';
