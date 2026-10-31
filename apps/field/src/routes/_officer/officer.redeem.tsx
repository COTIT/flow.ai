import { createFileRoute } from '@tanstack/react-router'

import { RedeemScreen } from '@/features/officer/RedeemScreen'

export const Route = createFileRoute('/_officer/officer/redeem')({
  component: RedeemScreen,
})
