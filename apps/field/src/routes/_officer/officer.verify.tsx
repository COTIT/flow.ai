import { createFileRoute } from '@tanstack/react-router'

import { VerifyQueueScreen } from '@/features/officer/VerifyQueueScreen'

export const Route = createFileRoute('/_officer/officer/verify')({
  component: VerifyQueueScreen,
})
