import { createFileRoute } from '@tanstack/react-router'

import { EquipmentListScreen } from '@/features/farmer/EquipmentListScreen'
import { validateResourceSearch } from '@/features/farmer/resourceKind'
import { guardResources } from '@/lib/features'

export const Route = createFileRoute('/_farmer/farm/equipment/')({
  // Held back by the release flag; a bookmark lands on the surface home.
  beforeLoad: guardResources('/farm'),
  // Equipment or loans, held in the URL so a link lands on the same list.
  validateSearch: validateResourceSearch,
  component: EquipmentListScreen,
})
