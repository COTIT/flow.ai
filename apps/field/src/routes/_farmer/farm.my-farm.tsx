import { createFileRoute } from '@tanstack/react-router'

import { MyFarmScreen } from '@/features/farmer/MyFarmScreen'

export const Route = createFileRoute('/_farmer/farm/my-farm')({
  component: MyFarmScreen,
})
