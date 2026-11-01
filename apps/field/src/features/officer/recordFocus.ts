import { isUuid } from '@/lib/ids'

export function validateFarmFocus(search: Record<string, unknown>): { plot?: string } {
  return typeof search.plot === 'string' && isUuid(search.plot) ? { plot: search.plot } : {}
}

export function validateCycleFocus(search: Record<string, unknown>): { harvest?: string } {
  return typeof search.harvest === 'string' && isUuid(search.harvest)
    ? { harvest: search.harvest }
    : {}
}
