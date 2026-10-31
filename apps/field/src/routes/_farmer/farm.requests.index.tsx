import { createFileRoute } from '@tanstack/react-router'

import { RequestsListScreen } from '@/features/farmer/RequestsListScreen'
import { guardResources } from '@/lib/features'

export const Route = createFileRoute('/_farmer/farm/requests/')({
  // Held back by the release flag; a bookmark lands on the surface home.
  beforeLoad: guardResources('/farm'),
  component: RequestsListScreen,
})
