import { createFileRoute } from '@tanstack/react-router'

import { OpsRequestsScreen } from '@/features/ops/OpsRequestsScreen'
import { validateRequestSearch } from '@/features/ops/requestSearch'
import { guardResources } from '@/lib/features'

export const Route = createFileRoute('/_ops/ops/requests/')({
  // Held back by the release flag; a bookmark lands on the surface home.
  beforeLoad: guardResources('/ops'),
  // Spec 7.2: filter state lives in the URL as VALIDATED search params, so a
  // stale bookmark degrades to the unfiltered view instead of breaking.
  validateSearch: validateRequestSearch,
  component: OpsRequestsScreen,
})
