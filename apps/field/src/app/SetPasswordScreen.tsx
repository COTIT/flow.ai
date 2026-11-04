import { useEffect, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { useQueryClient } from '@tanstack/react-query'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { useTranslation } from 'react-i18next'
import { z } from 'zod'

import { BrandLockup } from '@/app/BrandLockup'
import { resolveLanding } from '@/app/membership'
import { sessionQuery } from '@/app/session'
import { CONTROL_FIELD } from '@/components/controlStyles'
import { Card } from '@/components/controls'
import { BangMark } from '@/components/marks'
import { Button } from '@/components/ui/button'
import { clearTempPassword, peekTempPassword } from '@/app/tempPassword'
import { apiErrorMessage, isNoAnswer } from '@/lib/apiError'
import { errorText } from '@/lib/errors'
import { farmLoginsChangePassword } from '@genesy/api-client/endpoints/farm-logins'

// Only "is it there, and typed twice the same" is checked here. What makes a
// password acceptable is the auth server's rule, and its message says so.
const schema = z
  .object({
    // Only asked for when the sign-in password is no longer in memory (after a reload).
    current: z.string(),
    password: z.string().min(1, 'setPassword.required'),
    confirm: z.string().min(1, 'setPassword.required'),
  })
  .refine((values) => values.password === values.confirm, {
    path: ['confirm'],
    message: 'setPassword.mismatch',
  })

type FormValues = z.infer<typeof schema>

/**
 * A farmer's first sign-in uses the temporary password their officer read out
 * from the login card. Until they choose their own, every surface guard sends
 * them here — and the database refuses their survey answers, so the officer
 * who knows the temporary password cannot answer on their behalf.
 *
 * One call: `POST /v1/farm/password` checks the current (temporary) password,
 * applies the farmer password rule and clears `must_change_password`.
 */
export function SetPasswordScreen() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [formError, setFormError] = useState<string | null>(null)
  // Copied out of memory once, then cleared there: it lives only in this screen's state.
  const [heldCurrent] = useState(peekTempPassword)
  useEffect(() => clearTempPassword(), [])
  // The raw message is kept and read at render, so it follows a language
  // switch. A message the app names exactly is read from the bundle, machine
  // noise reads as the generic error, and any other sentence is shown as written.
  const shownError = formError ? errorText(formError, t) : null

  const {
    register,
    handleSubmit,
    formState: { isSubmitting, errors },
  } = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: { current: '' } })

  const onSubmit = async (values: FormValues) => {
    setFormError(null)
    try {
      const current = heldCurrent ?? values.current
      if (!current) {
        setFormError(t('setPassword.currentRequired'))
        return
      }
      await farmLoginsChangePassword({ current_password: current, password: values.password })
      const session = await queryClient.fetchQuery(sessionQuery)
      await navigate({ to: resolveLanding(session?.memberships ?? []).to, replace: true })
    } catch (cause) {
      setFormError(isNoAnswer(cause) ? t('error.network') : (apiErrorMessage(cause) ?? t('setPassword.unexpected')))
    }
  }

  const fieldError = (name: keyof FormValues) => {
    const key = errors[name]?.message
    return key ? t(key) : undefined
  }

  return (
    <section className="flex justify-center p-4 pt-6 sm:min-h-[75vh] sm:items-center sm:p-8">
      <Card className="flex w-full max-w-sm flex-col gap-4 p-6 sm:p-8">
        <div className="flex flex-col items-center gap-1 text-center">
          <BrandLockup height={34} />
          <h1 className="type-screen-title">{t('setPassword.title')}</h1>
          <p className="type-note" style={{ color: 'var(--ink-2)' }}>
            {t('setPassword.detail')}
          </p>
        </div>

        <form className="flex flex-col gap-3" onSubmit={handleSubmit(onSubmit)} noValidate>
          {heldCurrent === null && (
            <PasswordField
              id="set-password-current"
              label={t('setPassword.current')}
              error={fieldError('current')}
              registration={register('current')}
              autoComplete="current-password"
            />
          )}
          <PasswordField
            id="set-password-new"
            label={t('setPassword.password')}
            error={fieldError('password')}
            registration={register('password')}
          />
          <PasswordField
            id="set-password-confirm"
            label={t('setPassword.confirm')}
            error={fieldError('confirm')}
            registration={register('confirm')}
          />

          {formError && (
            <p
              data-testid="set-password-error"
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
              {shownError}
            </p>
          )}

          <Button
            type="submit"
            data-testid="set-password-submit"
            disabled={isSubmitting}
            variant="primary"
            size="lg"
            className="w-full"
          >
            {isSubmitting ? t('setPassword.submitting') : t('setPassword.submit')}
          </Button>
        </form>
      </Card>
    </section>
  )
}

function PasswordField({
  id,
  label,
  error,
  registration,
  autoComplete = 'new-password',
}: {
  id: string
  label: string
  error: string | undefined
  registration: ReturnType<ReturnType<typeof useForm<FormValues>>['register']>
  autoComplete?: string
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <label className="block" htmlFor={id} style={LABEL}>
        {label}
      </label>
      <input
        id={id}
        data-testid={id}
        type="password"
        autoComplete={autoComplete}
        aria-invalid={error ? true : undefined}
        aria-describedby={error ? `${id}-error` : undefined}
        style={error ? INVALID_FIELD : CONTROL_FIELD}
        {...registration}
      />
      {error && (
        <p
          id={`${id}-error`}
          data-testid={`${id}-error`}
          className="flex items-start gap-[7px] font-medium"
          style={FIELD_ERROR}
        >
          <BangMark />
          {error}
        </p>
      )}
    </div>
  )
}

const LABEL = { fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' } as const
const INVALID_FIELD = { ...CONTROL_FIELD, border: '1.5px solid var(--flag-ink)' }
const FIELD_ERROR = { fontSize: 13, color: 'var(--flag-ink)', textWrap: 'pretty' } as const
