import { createFileRoute } from '@tanstack/react-router'

import { DemandDetailScreen } from '@/features/ops/DemandDetailScreen'

export const Route = createFileRoute('/_ops/ops/demand/$demandId')({
  component: DemandDetailScreen,
})
