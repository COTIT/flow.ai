import { createFileRoute } from '@tanstack/react-router'

import { RequestDetailScreen } from '@/features/farmer/RequestDetailScreen'
import { guardResources } from '@/lib/features'

export const Route = createFileRoute('/_farmer/farm/requests/$requestId')({
  // Held back by the release flag; a bookmark lands on the surface home.
  beforeLoad: guardResources('/farm'),
  component: RequestDetailScreen,
})
