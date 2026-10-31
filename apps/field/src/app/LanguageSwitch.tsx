import { useEffect, useRef } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useRouterState } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useSession } from '@/app/session'
import { humanizeDbError } from '@/lib/errors'
import { supabase } from '@/lib/supabase'
import { queryKeys } from '@/lib/queryKeys'
import { isSupportedLanguage, supportedLanguages } from '@/i18n'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import { clsx as cx } from 'clsx'
/**
 * Spec 4.1: the language choice is persisted to `app_user.locale`.
 *
 * app_user.locale is the source of truth rather than localStorage, so the
 * choice follows the user to their next device. Signed out there is nothing to
 * persist to, and the switch is in-memory only.
 */
export function LanguageSwitch({ className = '' }: { className?: string }) {
  const { i18n, t } = useTranslation()
  const { data: session } = useSession()
  const queryClient = useQueryClient()

  const storedLocale = session?.appUser?.locale
  const appliedFor = useRef<string | null>(null)

  // Apply the stored locale once per user, not on every render: doing it
  // unconditionally would fight the user every time they picked a language.
  useEffect(() => {
    const userId = session?.appUser?.id ?? null
    if (!userId || !storedLocale) return
    if (appliedFor.current === userId) return
    appliedFor.current = userId
    if (isSupportedLanguage(storedLocale) && storedLocale !== i18n.resolvedLanguage) {
      void i18n.changeLanguage(storedLocale)
    }
  }, [session?.appUser?.id, storedLocale, i18n])

  const persist = useMutation({
    mutationFn: async (locale: string) => {
      const userId = session?.appUser?.id
      if (!userId) return
      // app_user_update_self: `id = auth.uid()`. Nobody else's locale is
      // writable, so there is nothing to guard client-side.
      const { error } = await supabase.from('app_user').update({ locale }).eq('id', userId)
      // Wrapped, not rethrown as-is: a PostgrestError is a plain object and
      // stringifies to [object Object], which would hide the message the
      // schema wrote to be read.
      if (error) throw new Error(error.message)
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.session() }),
  })

  const onChange = async (locale: string) => {
    // Clear any previous failure first, so a stale message cannot outlive the
    // attempt that replaces it.
    persist.reset()
    await i18n.changeLanguage(locale)
    if (!session?.appUser?.id) return
    // Awaited, and the control is disabled while it runs. Fire-and-forget lost
    // the write whenever the page navigated before the request finished, and
    // an unsaved change must not look saved.
    //
    // A failure is reported rather than swallowed: the language has already
    // changed on screen, so silence would let the user believe it was stored.
    try {
      await persist.mutateAsync(locale)
    } catch {
      // The message is rendered from the mutation's own error state below.
    }
  }

  /**
   * The banner describes one attempt, on one screen. It used to survive the
   * route change and two more after it, still describing an event that was
   * over (QA #24). This control lives in the shell header, so nothing else
   * unmounts it.
   */
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const reset = persist.reset
  useEffect(() => {
    reset()
  }, [pathname, reset])

  const failure = persist.error
  // `TypeError: Failed to fetch` told a field officer nothing (QA #25). The
  // honest half of the sentence — changed on screen, not stored — stays.
  const human = failure ? humanizeDbError(failure) : null
  const failureMessage =
    human === null ? undefined : human.kind === 'verbatim' ? human.message : t(human.key, human.values)

  return (
    <div className={cx(`inline-flex flex-col items-end gap-1`, className)}>
      <Select
          value={i18n.resolvedLanguage}
          onValueChange={(value) => void onChange(value ?? '')}
          disabled={persist.isPending}
        >
          <SelectTrigger
          data-testid="language-switch"
            aria-label={t('a11y.language')}
            className="min-h-10 w-full min-w-28 px-2.5 sm:w-auto"
          >
            {t(`language.${i18n.resolvedLanguage}`)}
          </SelectTrigger>
          <SelectContent>
            {supportedLanguages.map((lng) => (
              <SelectItem key={lng} value={lng}>
                {t(`language.${lng}`)}
              </SelectItem>
            ))}
          </SelectContent>
      </Select>
      {failureMessage && (
        <p
          data-testid="language-error"
          role="alert"
          className="type-note"
          style={{ color: 'var(--flag-ink)' }}
        >
          {t('language.notSaved')} {failureMessage}
        </p>
      )}
    </div>
  )
}
