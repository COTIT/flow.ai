import { createFileRoute } from '@tanstack/react-router'

import { TowerMarketScreen } from '@/features/tower/TowerDrillScreens'
import { validateVillageSearch } from '@/features/tower/towerSearch'

export const Route = createFileRoute('/_ops/ops/tower/market')({
  validateSearch: validateVillageSearch,
  component: TowerMarketScreen,
})
