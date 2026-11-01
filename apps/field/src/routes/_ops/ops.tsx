import { Outlet, createFileRoute } from '@tanstack/react-router'

// The Control Tower lives inside _ops at /ops/tower, not in a fifth route
// group: executive is an Ops view in the MVP, not a fourth role.
export const Route = createFileRoute('/_ops/ops')({
  component: () => <Outlet />,
})
