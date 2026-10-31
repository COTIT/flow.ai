import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const rpc = vi.fn()
vi.mock('@/lib/supabase', () => ({ supabase: { rpc: (...args: unknown[]) => rpc(...args) } }))

const { useActorName } = await import('@/lib/actorNames')

const OFFICER = '80000000-0000-4000-8000-000000000003'

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>
}

beforeEach(() => rpc.mockReset())

describe('useActorName', () => {
  test('resolves a visible actor through the scoped RPC, never app_user', async () => {
    rpc.mockResolvedValue({ data: [{ id: OFFICER, display_name: 'Salima Officer' }], error: null })
    const { result } = renderHook(() => useActorName(OFFICER), { wrapper })
    await waitFor(() => expect(result.current).toBe('Salima Officer'))
    expect(rpc).toHaveBeenCalledWith('app_actor_names', { p_ids: [OFFICER] })
  })

  // Zero rows is an answer: the caller may not see that name. Show nothing
  // rather than the raw id.
  test('an actor outside the caller scope resolves to nothing', async () => {
    rpc.mockResolvedValue({ data: [], error: null })
    const { result } = renderHook(() => useActorName(OFFICER), { wrapper })
    await waitFor(() => expect(rpc).toHaveBeenCalled())
    expect(result.current).toBeUndefined()
  })

  test('no actor, no lookup', () => {
    const { result } = renderHook(() => useActorName(null), { wrapper })
    expect(result.current).toBeUndefined()
    expect(rpc).not.toHaveBeenCalled()
  })

  test('a failed lookup shows nothing instead of an id', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'offline' } })
    const { result } = renderHook(() => useActorName(OFFICER), { wrapper })
    await waitFor(() => expect(rpc).toHaveBeenCalled())
    expect(result.current).toBeUndefined()
  })
})
