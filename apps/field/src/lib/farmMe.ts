import { apiRequest } from '@genesy/api-client/mutator'

// shortcut: hand-written until GET/PATCH /v1/farm/me is in the generated client (Phase 4 step 7);
// then this file is replaced by the generated call and model.

export type FarmRole = 'farmer' | 'field_officer' | 'ops' | 'admin'

export interface FarmMeMembership {
  id: string
  workspace_id: string
  workspace_name: string
  role: FarmRole
  village_id: string | null
  village_name: string | null
  person_id: string | null
}

export interface FarmMe {
  user: { id: string; name: string; email: string | null; phone: string | null }
  /** nl is shown as en until RUAHA has Dutch strings. */
  locale: 'sw' | 'en' | 'nl' | null
  must_change_password: boolean
  /** The caller's own live roles, every workspace. */
  memberships: FarmMeMembership[]
}

export const getFarmMe = () =>
  apiRequest<{ data: FarmMe }>({ url: '/farm/me', method: 'GET' }).then((r) => r.data)

export const updateFarmMe = (body: { locale: 'sw' | 'en' | 'nl' }) =>
  apiRequest<void>({ url: '/farm/me', method: 'PATCH', data: body })
