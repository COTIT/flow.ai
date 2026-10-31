import { createFileRoute } from '@tanstack/react-router'

import { EquipmentDetailScreen } from '@/features/farmer/EquipmentDetailScreen'
import { guardResources } from '@/lib/features'

export const Route = createFileRoute('/_farmer/farm/equipment/$equipmentId')({
  // Held back by the release flag; a bookmark lands on the surface home.
  beforeLoad: guardResources('/farm'),
  component: EquipmentDetailScreen,
})
