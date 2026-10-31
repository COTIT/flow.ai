import { createFileRoute } from '@tanstack/react-router'

import { OfficerCycleScreen } from '@/features/officer/OfficerRecordScreens'
import { validateCycleFocus } from '@/features/officer/recordFocus'

export const Route = createFileRoute('/_officer/officer/cycles/$cycleId')({
  validateSearch: validateCycleFocus,
  component: OfficerCycleScreen,
})
