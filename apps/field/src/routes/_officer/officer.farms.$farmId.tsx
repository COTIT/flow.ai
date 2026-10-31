import { createFileRoute } from '@tanstack/react-router'

import { OfficerFarmScreen } from '@/features/officer/OfficerRecordScreens'
import { validateFarmFocus } from '@/features/officer/recordFocus'

export const Route = createFileRoute('/_officer/officer/farms/$farmId')({
  validateSearch: validateFarmFocus,
  component: OfficerFarmScreen,
})
