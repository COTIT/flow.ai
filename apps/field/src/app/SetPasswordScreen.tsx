import { useState } from 'react'
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
import { translateDbMessage } from '@/lib/dbMessages'
import { supabase } from '@/lib/supabase'

// Only "is it there, and typed twice the same" is checked here. What makes a
// password acceptable is the auth server's rule, and its message says so.
const schema = z
  .object({
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
 * Two steps, in this order: the auth server stores the new password, then
 * app_password_changed checks the stored hash is no longer the temporary one
 * and clears the flag. The second cannot be faked by skipping the first.
 */
export function SetPasswordScreen() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [formError, setFormError] = useState<string | null>(null)
  // The raw message is kept and read at render, so it follows a language
  // switch. A message the app names exactly is read from the bundle; any other
  // (a GoTrue sentence, say) is shown as written.
  const named = formError ? translateDbMessage(formError) : null
  const shownError = named ? t(named.key, named.values) : formError

  const {
    register,
    handleSubmit,
    formState: { isSubmitting, errors },
  } = useForm<FormValues>({ resolver: zodResolver(schema) })

  const onSubmit = async (values: FormValues) => {
    setFormError(null)
    try {
      const updated = await supabase.auth.updateUser({ password: values.password })
      if (updated.error) {
        setFormError(updated.error.message)
        return
      }
      const changed = await supabase.rpc('app_password_changed')
      if (changed.error) {
        setFormError(changed.error.message)
        return
      }
      const session = await queryClient.fetchQuery(sessionQuery)
      await navigate({ to: resolveLanding(session?.memberships ?? []).to, replace: true })
    } catch (cause) {
      setFormError(cause instanceof Error ? cause.message : t('setPassword.unexpected'))
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
}: {
  id: string
  label: string
  error: string | undefined
  registration: ReturnType<ReturnType<typeof useForm<FormValues>>['register']>
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
        autoComplete="new-password"
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
