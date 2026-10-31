import { createFileRoute } from '@tanstack/react-router'

import { TowerScreen } from '@/features/tower/TowerScreen'
import { validateVillageSearch } from '@/features/tower/towerSearch'

export const Route = createFileRoute('/_ops/ops/tower/')({
  validateSearch: validateVillageSearch,
  component: TowerScreen,
})
