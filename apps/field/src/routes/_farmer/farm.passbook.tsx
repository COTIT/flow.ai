import { createFileRoute } from '@tanstack/react-router'

import { MyPassbookScreen } from '@/features/passbook/screens'

export const Route = createFileRoute('/_farmer/farm/passbook')({
  component: MyPassbookScreen,
})
