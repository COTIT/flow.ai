import { createFileRoute } from '@tanstack/react-router'

import { TowerProductionScreen } from '@/features/tower/TowerDrillScreens'
import { validateVillageSearch } from '@/features/tower/towerSearch'

export const Route = createFileRoute('/_ops/ops/tower/production')({
  validateSearch: validateVillageSearch,
  component: TowerProductionScreen,
})
