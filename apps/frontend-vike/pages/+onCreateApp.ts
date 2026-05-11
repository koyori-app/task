// https://vike.dev/onCreateApp
export { onCreateApp }

import type { PageContext } from 'vike/types'
import { createHead } from '@unhead/vue/client'

function onCreateApp(pageContext: PageContext) {
  if (pageContext.isRenderingHead) return
  const app = pageContext.app!
  const head = createHead()
  app.use(head)
}