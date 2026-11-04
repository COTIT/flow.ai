import { QueryClient } from '@tanstack/react-query'
import { sessionStore } from '@genesy/api-client/session'
import { afterEach, describe, expect, test, vi } from 'vitest'

import { returnToSignInWhenExpired } from '@/app/sessionExpiry'

// B5: a 401 mid-session signs out in the API client; the app has to leave the dead screen.
describe('returnToSignInWhenExpired', () => {
  let stop: () => void = () => {}
  afterEach(() => stop())

  test('a session the API ended goes to sign-in, keeping where it was, and forgets cached data', () => {
    const navigate = vi.fn()
    const queryClient = new QueryClient()
    queryClient.setQueryData(['people'], ['Neema'])
    sessionStore.setState({ token: 'live', expired: false })
    stop = returnToSignInWhenExpired(queryClient, { navigate, href: () => '/officer/people' })

    sessionStore.getState().signOut('expired')

    expect(navigate).toHaveBeenCalledWith({ to: '/login', search: { redirect: '/officer/people' }, replace: true })
    expect(queryClient.getQueryData(['people'])).toBeUndefined()
  })

  test("the user's own sign-out is left to the sign-out button", () => {
    const navigate = vi.fn()
    sessionStore.setState({ token: 'live', expired: false })
    stop = returnToSignInWhenExpired(new QueryClient(), { navigate, href: () => '/officer' })

    sessionStore.getState().signOut()

    expect(navigate).not.toHaveBeenCalled()
  })
})
