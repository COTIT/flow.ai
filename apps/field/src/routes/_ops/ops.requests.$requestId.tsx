import { createFileRoute } from '@tanstack/react-router'

import { OpsRequestReviewScreen } from '@/features/ops/OpsRequestReviewScreen'
import { guardResources } from '@/lib/features'

export const Route = createFileRoute('/_ops/ops/requests/$requestId')({
  // Held back by the release flag; a bookmark lands on the surface home.
  beforeLoad: guardResources('/ops'),
  component: OpsRequestReviewScreen,
})
