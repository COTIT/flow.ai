import { createFileRoute } from '@tanstack/react-router'

import { TowerEnergyScreen } from '@/features/tower/TowerDrillScreens'
import { validateVillageSearch } from '@/features/tower/towerSearch'

export const Route = createFileRoute('/_ops/ops/tower/energy')({
  validateSearch: validateVillageSearch,
  component: TowerEnergyScreen,
})
