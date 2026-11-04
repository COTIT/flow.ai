import { act, fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useBatchOptions = vi.fn()
type Mutation = { mutate: ReturnType<typeof vi.fn>; isPending: boolean; error: unknown; isSuccess: boolean; reset: ReturnType<typeof vi.fn> }
const mutation = (): Mutation => ({ mutate: vi.fn(), isPending: false, error: null, isSuccess: false, reset: vi.fn() })
let createRaw = mutation()
let createBatch = mutation()

vi.mock('@/features/ops/supply/useSupply', () => ({
  useBatchOptions: (village: string, crop: string) => useBatchOptions(village, crop),
  useCreateRawProduct: () => createRaw,
  useCreateBatch: () => createBatch,
}))
vi.mock('@/features/officer/useCrops', () => ({
  useCrops: () => ({ isLoading: false, error: null, crops: [{ id: 'crop1', name: 'Maize' }] }),
}))
vi.mock('@/features/ops/useOpsReference', () => ({
  useVillageCapacity: () => ({ isLoading: false, error: null, data: [{ id: 'v1', name: 'Ilundo' }] }),
}))
vi.mock('@/features/ops/surveys/useSurveyAdmin', () => ({
  useCurrencies: () => ({
    isLoading: false,
    error: null,
    data: [{ id: 'tzs', name: 'Tanzanian shilling', decimals: 2, default_currency: true }],
  }),
}))
vi.mock('@/lib/ids', () => ({ newUuid: () => 'ref-1' }))

const { BuyHarvestCard } = await import('@/features/ops/supply/BuyHarvestCard')

const harvest = { harvest_report_id: 'h1', farmer_name: 'Neema Mwakalinga', plot_label: 'Plot A', quantity_kg: '300.00', committed_kg: '100.00', available_kg: '200.00' }
const product = (id: string, name: string, suggested: boolean) => ({ id, name, unit: 'kg', suggested_for_crop: suggested })
const options = (over: Record<string, unknown> = {}) => ({
  isLoading: false,
  error: null,
  data: { harvests: [harvest], raw_products: [product('p1', 'Honey jar', false)], can_create_product: true, ...over },
})
const refused = (errors: Record<string, string[]>) => ({ isAxiosError: true, response: { status: 422, data: { message: 'x', errors } } })

beforeEach(() => {
  vi.clearAllMocks()
  createRaw = mutation()
  createBatch = mutation()
  useBatchOptions.mockReturnValue(options())
})

const choose = () => {
  fireEvent.change(screen.getByTestId('batch-village'), { target: { value: 'v1' } })
  fireEvent.change(screen.getByTestId('batch-crop'), { target: { value: 'crop1' } })
}

describe('the guided raw product step', () => {
  test('asks nothing until a village and a crop are chosen', () => {
    useBatchOptions.mockReturnValue({ isLoading: false, error: null, data: undefined })
    render(<BuyHarvestCard />)
    expect(useBatchOptions).toHaveBeenLastCalledWith('', '')
    expect(screen.queryByTestId('batch-no-raw-product')).toBeNull()
  })

  test('no product suggested for the crop: an info notice with a create button', () => {
    render(<BuyHarvestCard />)
    choose()
    expect(useBatchOptions).toHaveBeenLastCalledWith('v1', 'crop1')
    const notice = screen.getByTestId('batch-no-raw-product')
    expect(notice).toHaveTextContent('No raw product for Maize yet')
    expect(screen.getByTestId('batch-create-raw')).toBeVisible()
  })

  test('a product suggested for the crop: no notice, and it is listed first', () => {
    useBatchOptions.mockReturnValue(options({ raw_products: [product('p2', 'Raw maize', true), product('p1', 'Honey jar', false)] }))
    render(<BuyHarvestCard />)
    choose()
    expect(screen.queryByTestId('batch-no-raw-product')).toBeNull()
    const labels = [...(screen.getByTestId('batch-product') as HTMLSelectElement).options].map((o) => o.text)
    expect(labels.slice(1)).toEqual(['Raw maize (kg)', 'Honey jar (kg)'])
  })

  test('without the right: the notice says who can add one, no button', () => {
    useBatchOptions.mockReturnValue(options({ raw_products: [], can_create_product: false }))
    render(<BuyHarvestCard />)
    choose()
    expect(screen.getByTestId('batch-no-raw-product')).toHaveTextContent('Office → Products')
    expect(screen.queryByTestId('batch-create-raw')).toBeNull()
  })

  test('the dialog is pre-filled, editable, and confirming selects the new product', () => {
    render(<BuyHarvestCard />)
    choose()
    fireEvent.click(screen.getByTestId('batch-create-raw'))
    const name = screen.getByTestId('raw-product-name') as HTMLInputElement
    expect(name.value).toBe('Raw Maize')
    expect(screen.getByTestId('confirm-dialog')).toHaveTextContent('kg')
    fireEvent.change(name, { target: { value: 'Raw maize grain' } })
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))

    expect(createRaw.mutate).toHaveBeenCalledWith('Raw maize grain', expect.anything())
    useBatchOptions.mockReturnValue(options({ raw_products: [product('p1', 'Honey jar', false), product('p9', 'Raw maize grain', true)] }))
    const { onSuccess } = createRaw.mutate.mock.calls[0]![1] as { onSuccess: (p: unknown) => void }
    act(() => onSuccess({ id: 'p9', name: 'Raw maize grain', unit: 'kg' }))
    expect(screen.queryByTestId('confirm-dialog')).toBeNull()
    expect((screen.getByTestId('batch-product') as HTMLSelectElement).value).toBe('p9')
  })

  test('a refused name shows under the field and the dialog stays', () => {
    createRaw.error = refused({ name: ['The name field is required.'] })
    render(<BuyHarvestCard />)
    choose()
    fireEvent.click(screen.getByTestId('batch-create-raw'))
    expect(screen.getByTestId('raw-product-name-error')).toHaveTextContent('The name field is required.')
    expect(screen.getByTestId('confirm-dialog')).toBeVisible()
  })
})

describe('buying', () => {
  const fill = () => {
    choose()
    fireEvent.change(screen.getByTestId('batch-product'), { target: { value: 'p1' } })
    fireEvent.change(screen.getByTestId('batch-code'), { target: { value: 'ILU-M-01' } })
    fireEvent.change(screen.getByTestId('batch-price'), { target: { value: '8000' } })
  }

  test('lines start at the kg left; confirm posts minor units under one client_ref', () => {
    render(<BuyHarvestCard />)
    fill()
    expect((screen.getByTestId('batch-kg-h1') as HTMLInputElement).value).toBe('200.00')
    expect(screen.getByTestId('batch-line-h1')).toHaveTextContent('Neema Mwakalinga')
    fireEvent.click(screen.getByTestId('batch-submit'))
    fireEvent.click(screen.getByTestId('confirm-dialog-confirm'))

    expect(createBatch.mutate).toHaveBeenCalledWith(
      {
        client_ref: 'ref-1',
        village_id: 'v1',
        crop_id: 'crop1',
        code: 'ILU-M-01',
        raw_product_id: 'p1',
        price_per_kg: 800000,
        currency_id: 'tzs',
        lines: [{ harvest_report_id: 'h1', kg: '200.00' }],
      },
      expect.anything(),
    )
  })

  test('a blank kg leaves the line out; nothing to buy keeps the button off', () => {
    render(<BuyHarvestCard />)
    fill()
    fireEvent.change(screen.getByTestId('batch-kg-h1'), { target: { value: '' } })
    expect(screen.getByTestId('batch-submit')).toBeDisabled()
  })

  test('a price that is not a number keeps the button off', () => {
    render(<BuyHarvestCard />)
    fill()
    fireEvent.change(screen.getByTestId('batch-price'), { target: { value: '8,000.555' } })
    expect(screen.getByTestId('batch-submit')).toBeDisabled()
  })

  test("the API's refusal of a line shows under that line", () => {
    createBatch.error = refused({ 'lines.0.kg': ['over-commitment: 200.00 kg available'] })
    render(<BuyHarvestCard />)
    fill()
    expect(screen.getByTestId('batch-kg-h1-error')).toHaveTextContent('over-commitment')
  })

  test('no harvests to buy: an empty state instead of lines', () => {
    useBatchOptions.mockReturnValue(options({ harvests: [] }))
    render(<BuyHarvestCard />)
    choose()
    expect(screen.getByTestId('batch-no-harvests')).toBeVisible()
  })
})
