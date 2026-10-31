import { useState } from 'react'
import { getRouteApi, useNavigate } from '@tanstack/react-router'
import { useQueryClient } from '@tanstack/react-query'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { useTranslation } from 'react-i18next'

import { BrandLockup } from '@/app/BrandLockup'
import { CONTROL_FIELD } from '@/components/controlStyles'
import { Card } from '@/components/controls'
import { BangMark } from '@/components/marks'
import { Button } from '@/components/ui/button'
import { z } from 'zod'

import { resolveLanding, safeRedirect } from '@/app/membership'
import { sessionQuery } from '@/app/session'
import { loginEmailFor } from '@/lib/phone'
import { supabase } from '@/lib/supabase'

// By route id rather than by importing the route, which would be circular.
const route = getRouteApi('/(auth)/login')

// Messages are i18n keys, resolved at render. Validation is client-side only
// because it costs a round trip to learn an empty field is empty — it is not a
// copy of any server rule. Staff type an email; farmers type the phone their
// officer registered, which maps to the hidden address their login was
// created under (src/lib/phone.ts).
const schema = z.object({
  email: z
    .string()
    .min(1, 'login.emailRequired')
    .refine((value) => loginEmailFor(value) !== null, 'login.emailInvalid'),
  password: z.string().min(1, 'login.passwordRequired'),
})

type FormValues = z.infer<typeof schema>

export function LoginScreen() {
  const { redirect: requested } = route.useSearch()
  const { t } = useTranslation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [formError, setFormError] = useState<string | null>(null)

  const {
    register,
    handleSubmit,
    formState: { isSubmitting, errors },
  } = useForm<FormValues>({ resolver: zodResolver(schema) })

  const onSubmit = async (values: FormValues) => {
    setFormError(null)

    try {
      const { error } = await supabase.auth.signInWithPassword({
        // Never null here: the schema refused anything loginEmailFor rejects.
        email: loginEmailFor(values.email) as string,
        password: values.password,
      })

      if (error) {
        // Three distinguishable states, per spec 4.2: bad credentials, an
        // unreachable server, and anything else — which is surfaced verbatim
        // because the messages in this system are written to be read.
        if (error.status === 400) setFormError(t('login.invalid'))
        else if (error.status === undefined) setFormError(t('login.network'))
        else setFormError(error.message)
        return
      }

      /**
       * A forced read, not `invalidateQueries` + `ensureQueryData` — QA #30.
       *
       * Every route guard calls `ensureSession`, so by the time this form is
       * submitted the session query already holds `null`: the correct answer
       * for a visitor who was signed out. `invalidateQueries` marks that stale
       * and STARTS a refetch without waiting for it, and `ensureQueryData`
       * returns cached data whenever there is any — and `null` is data. So
       * `resolveLanding([])` ran on the signed-out answer and sent a
       * legitimate ops user to "You do not have access", intermittently,
       * depending on which promise settled first.
       *
       * `fetchQuery` ignores what is cached and returns the fresh answer.
       *
       * The password can also be accepted and this read still fail; that is
       * reported rather than swallowed, or the user is left staring at the
       * login form having just typed a correct password.
       */
      const session = await queryClient.fetchQuery(sessionQuery)

      // A guard that bounced someone here attached where they were going. Go
      // back there if the value is trustworthy; the surface guard will correct
      // it if their role does not open that surface. Otherwise, role home.
      const target = safeRedirect(requested) ?? resolveLanding(session?.memberships ?? []).to
      await navigate({ to: target, replace: true })
    } catch (cause) {
      // signInWithPassword can reject outright, not just resolve with an
      // error — a rejection here must not escape into an unhandled promise.
      setFormError(cause instanceof Error ? cause.message : t('login.unexpected'))
    }
  }

  const fieldError = (name: keyof FormValues) => {
    const key = errors[name]?.message
    return key ? t(key) : undefined
  }

  const emailError = fieldError('email')
  const passwordError = fieldError('password')

  return (
    <section className="flex justify-center p-4 pt-6 sm:min-h-[75vh] sm:items-center sm:p-8">
      <Card className="flex w-full max-w-sm flex-col gap-4 p-6 sm:p-8">
        <div className="flex flex-col items-center gap-1 text-center">
          <BrandLockup height={34} />
          <h1 className="type-screen-title">{t('login.title')}</h1>
        </div>

        <form className="flex flex-col gap-3" onSubmit={handleSubmit(onSubmit)} noValidate>
          <div className="flex flex-col gap-1.5">
            <label className="block" htmlFor="login-email" style={LABEL}>
              {t('login.email')}
            </label>
            <input
              id="login-email"
              data-testid="login-email"
              type="text"
              autoComplete="username"
              aria-invalid={emailError ? true : undefined}
              aria-describedby={emailError ? 'login-email-error' : undefined}
              style={emailError ? INVALID_FIELD : CONTROL_FIELD}
              {...register('email')}
            />
            {emailError && (
              <p
                id="login-email-error"
                data-testid="login-email-error"
                className="flex items-start gap-[7px] font-medium"
                style={FIELD_ERROR}
              >
                <BangMark />
                {emailError}
              </p>
            )}
          </div>

          <div className="flex flex-col gap-1.5">
            <label className="block" htmlFor="login-password" style={LABEL}>
              {t('login.password')}
            </label>
            <input
              id="login-password"
              data-testid="login-password"
              type="password"
              autoComplete="current-password"
              aria-invalid={passwordError ? true : undefined}
              aria-describedby={passwordError ? 'login-password-error' : undefined}
              style={passwordError ? INVALID_FIELD : CONTROL_FIELD}
              {...register('password')}
            />
            {passwordError && (
              <p
                id="login-password-error"
                data-testid="login-password-error"
                className="flex items-start gap-[7px] font-medium"
                style={FIELD_ERROR}
              >
                <BangMark />
                {passwordError}
              </p>
            )}
          </div>

          {formError && (
            <p
              data-testid="login-error"
              role="alert"
              className="flex items-start gap-2 px-3.5 py-3"
              style={{
                border: '1px solid rgba(158, 27, 27, .25)',
                borderLeft: '4px solid var(--flag-ink)',
                borderRadius: 'var(--radius-card)',
                background: 'var(--flag-tint)',
                fontSize: 14,
                lineHeight: 1.55,
                color: 'var(--ink)',
                textWrap: 'pretty',
              }}
            >
              <BangMark size={18} />
              {formError}
            </p>
          )}

          <Button
            type="submit"
            data-testid="login-submit"
            disabled={isSubmitting}
            variant="primary"
            size="lg"
            className="w-full"
          >
            {isSubmitting ? t('login.submitting') : t('login.submit')}
          </Button>
        </form>
      </Card>
    </section>
  )
}

const LABEL = { fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' } as const

const INVALID_FIELD = { ...CONTROL_FIELD, border: '1.5px solid var(--flag-ink)' }

const FIELD_ERROR = { fontSize: 13, color: 'var(--flag-ink)', textWrap: 'pretty' } as const
