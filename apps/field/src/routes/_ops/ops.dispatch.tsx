import { createFileRoute } from '@tanstack/react-router'

import { DispatchScreen } from '@/features/ops/supply/DispatchScreen'

export const Route = createFileRoute('/_ops/ops/dispatch')({
  component: DispatchScreen,
})
