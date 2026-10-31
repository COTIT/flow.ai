import { Outlet, createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_officer/officer')({
  component: () => <Outlet />,
})
