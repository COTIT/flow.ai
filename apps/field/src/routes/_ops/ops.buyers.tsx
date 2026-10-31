import { createFileRoute } from '@tanstack/react-router'

import { BuyersScreen } from '@/features/ops/BuyersScreen'

export const Route = createFileRoute('/_ops/ops/buyers')({
  component: BuyersScreen,
})
