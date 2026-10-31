import { farmLoginsMe, farmLoginsUpdateMe } from '@genesy/api-client/endpoints/farm-logins'
import type { FarmMe } from '@genesy/api-client/model'

export type { FarmMe }

/** The four farm roles; the spec types `role` as text. */
export type FarmRole = 'farmer' | 'field_officer' | 'ops' | 'admin'

/** `GET /v1/farm/me`: who is signed in, their own live farm roles, and the password flag. */
export const getFarmMe = () => farmLoginsMe().then((r) => r.data)

/** `PATCH /v1/farm/me`: the account's interface language (nl is shown as en until RUAHA has Dutch). */
export const updateFarmMe = (body: { locale: 'sw' | 'en' | 'nl' }) => farmLoginsUpdateMe(body)
