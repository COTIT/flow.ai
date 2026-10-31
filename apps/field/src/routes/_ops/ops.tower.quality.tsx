import { createFileRoute } from '@tanstack/react-router'

import { TowerQualityScreen } from '@/features/tower/TowerQualityScreen'
import { validateQualitySearch } from '@/features/tower/towerSearch'

export const Route = createFileRoute('/_ops/ops/tower/quality')({
  validateSearch: validateQualitySearch,
  component: TowerQualityScreen,
})
