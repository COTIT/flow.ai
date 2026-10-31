import { createFileRoute } from '@tanstack/react-router'

import { CatalogueScreen } from '@/features/ops/CatalogueScreen'
import { validateResourceSearch } from '@/features/farmer/resourceKind'
import { guardResources } from '@/lib/features'

export const Route = createFileRoute('/_ops/ops/catalogue')({
  // Held back by the release flag; a bookmark lands on the surface home.
  beforeLoad: guardResources('/ops'),
  // Equipment or loans, held in the URL so a link lands on the same list.
  validateSearch: validateResourceSearch,
  component: CatalogueScreen,
})
