import { Outlet, createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_ops/ops/tower')({
  component: () => <Outlet />,
})
