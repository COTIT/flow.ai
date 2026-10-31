import { createFileRoute } from '@tanstack/react-router'

import { OpsHomeScreen } from '@/features/ops/OpsHomeScreen'

export const Route = createFileRoute('/_ops/ops/')({
  component: OpsHomeScreen,
})
