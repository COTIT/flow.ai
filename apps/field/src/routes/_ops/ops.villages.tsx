import { createFileRoute } from '@tanstack/react-router'

import { VillagesScreen } from '@/features/ops/VillagesScreen'

export const Route = createFileRoute('/_ops/ops/villages')({
  component: VillagesScreen,
})
