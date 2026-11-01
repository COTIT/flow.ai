import { createFileRoute } from '@tanstack/react-router'

import { NoAccessScreen } from '@/app/NoAccessScreen'

export const Route = createFileRoute('/(auth)/no-access')({
  component: NoAccessScreen,
})
