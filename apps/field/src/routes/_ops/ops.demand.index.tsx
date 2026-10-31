import { createFileRoute } from '@tanstack/react-router'

import { DemandListScreen } from '@/features/ops/DemandListScreen'

export const Route = createFileRoute('/_ops/ops/demand/')({
  component: DemandListScreen,
})
