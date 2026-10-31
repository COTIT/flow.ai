import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

/**
 * An EarlierVersionSavedError means the earlier submission DID save. The form
 * tells the user to open it from the list, so the list caches must be
 * invalidated exactly as they are on success — or the list shown beside that
 * message is the stale one without the record.
 */

const insertResult = vi.fn()
function insertChain() {
  const result = () => insertResult() as Promise<unknown>
  return {
    select: () => ({ single: result }),
    then: (resolve: (value: unknown) => unknown, reject: (reason: unknown) => unknown) =>
      result().then(resolve, reject),
  }
}
vi.mock('@/lib/supabase', () => ({
  supabase: { from: () => ({ insert: () => insertChain() }) },
}))

const recoverInsert = vi.fn()
vi.mock('@/lib/recoverInsert', () => ({
  recoverInsert: (...args: unknown[]) => recoverInsert(...args) as unknown,
}))

const { EarlierVersionSavedError } = await import('@/lib/drafts')
const { useSubmitRequest } = await import('@/features/farmer/useRequests')
const { useCreateDemand } = await import('@/features/ops/useDemand')
const { useCreateBuyer } = await import('@/features/ops/useOpsReference')
const { useAttachSupply } = await import('@/features/ops/useOpportunity')

const ID = '11111111-1111-4111-8111-111111111111'
const VILLAGE = '22222222-2222-4222-8222-222222222222'
const OPPORTUNITY = '33333333-3333-4333-8333-333333333333'

type Filters = { queryKey?: unknown; predicate?: (query: { queryKey: readonly unknown[] }) => boolean }

/** Predicates are fresh closures per call; compare them by what they match. */
function normalise(calls: unknown[][]) {
  return calls.map(([filters]) => {
    const { predicate, ...rest } = (filters ?? {}) as Filters
    if (!predicate) return rest
    return {
      ...rest,
      predicate: [
        predicate({ queryKey: ['tower', 'energy', VILLAGE] }),
        predicate({ queryKey: ['tower', 'energy', 'another-village'] }),
      ],
    }
  })
}

interface Case {
  name: string
  mutate: (client: QueryClient) => Promise<unknown>
}

function run(useHook: () => { mutateAsync: (input: never) => Promise<unknown> }, input: unknown) {
  return (client: QueryClient) => {
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    )
    const { result } = renderHook(useHook, { wrapper })
    return result.current.mutateAsync(input as never)
  }
}

const cases: Case[] = [
  {
    name: 'useSubmitRequest',
    mutate: run(useSubmitRequest, {
      id: ID,
      actorId: 'actor',
      villageId: VILLAGE,
      personId: 'person',
      equipmentId: 'equipment',
      quantity: 1,
      hoursPerDay: 4,
      daysPerWeek: 5,
      purpose: '',
    }),
  },
  {
    name: 'useCreateDemand',
    mutate: run(useCreateDemand, {
      id: ID,
      projectId: 'project',
      buyerId: 'buyer',
      cropId: 'crop',
      quantityKg: 9000,
      windowStart: '2026-09-01',
      windowEnd: '2026-09-30',
      deliveryPoint: '',
      pricePerKg: '',
      qualityNote: '',
    }),
  },
  {
    name: 'useCreateBuyer',
    mutate: run(useCreateBuyer, {
      id: ID,
      project_id: 'project',
      name: 'Mbeya Millers',
      channel: 'direct',
      contact_note: null,
    }),
  },
  {
    name: 'useAttachSupply',
    mutate: run(() => useAttachSupply(OPPORTUNITY, VILLAGE), {
      id: ID,
      harvestReportId: 'harvest',
      cropCycleId: 'cycle',
      contributedKg: 100,
    }),
  },
]

function spiedClient() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const spy = vi.spyOn(client, 'invalidateQueries')
  return { client, spy }
}

async function successInvalidations(testCase: Case) {
  insertResult.mockResolvedValue({ data: { id: ID }, error: null })
  const { client, spy } = spiedClient()
  await testCase.mutate(client)
  expect(recoverInsert).not.toHaveBeenCalled()
  return normalise(spy.mock.calls)
}

beforeEach(() => {
  insertResult.mockReset()
  recoverInsert.mockReset()
})

describe.each(cases)('$name after a retried create', (testCase) => {
  test('an earlier version saved invalidates the same caches as a success', async () => {
    const onSuccess = await successInvalidations(testCase)
    expect(onSuccess.length).toBeGreaterThan(0)

    insertResult.mockResolvedValue({ data: null, error: { message: 'duplicate key' } })
    recoverInsert.mockRejectedValue(new EarlierVersionSavedError())
    const { client, spy } = spiedClient()

    await expect(testCase.mutate(client)).rejects.toBeInstanceOf(EarlierVersionSavedError)
    expect(normalise(spy.mock.calls)).toEqual(onSuccess)
  })

  test('an ordinary failure invalidates nothing', async () => {
    insertResult.mockResolvedValue({ data: null, error: { message: 'guard refused' } })
    recoverInsert.mockRejectedValue(new Error('guard refused'))
    const { client, spy } = spiedClient()

    await expect(testCase.mutate(client)).rejects.toThrow('guard refused')
    expect(spy).not.toHaveBeenCalled()
  })
})
