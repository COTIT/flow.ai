import { createRootRouteWithContext } from '@tanstack/react-router'

import { RootLayout } from '@/app/RootLayout'
import { RouteError, RouteNotFound } from '@/app/RouteBoundary'
import type { RouterContext } from '@/app/router'

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
  // A guard that cannot resolve the session throws. Without a boundary the
  // router renders its default error page and the message is lost; the schema's
  // errors are written to be read, so they get surfaced verbatim.
  errorComponent: RouteError,
  notFoundComponent: RouteNotFound,
})
