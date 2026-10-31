import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { activeMemberships, roleHome } from '@/app/membership'
import { useScopeNames } from '@/app/scope'
import { useSession } from '@/app/session'
import { EmptyState } from '@/components/EmptyState'
import { Loading } from '@/components/controls'
import { ErrorState } from '@/components/ErrorState'

/**
 * Spec 4.3: lists memberships as role + project + village. Selection is held
 * in memory and the URL, never in a token.
 */
export function SelectRoleScreen() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const session = useSession()
  const scope = useScopeNames()

  if (session.error) return <ErrorState error={session.error} />
  if (scope.error) return <ErrorState error={scope.error} />

  // Wait for the names too, so rows do not appear and then relabel themselves.
  if (session.isLoading || scope.isLoading || !session.data || !scope.data) {
    return <Loading testId="select-role-loading" />
  }

  const memberships = activeMemberships(session.data.memberships)

  // Landing here with nothing to pick means the memberships were revoked
  // between resolving the route and rendering it. Say so rather than showing
  // an empty list.
  if (memberships.length === 0) {
    return <EmptyState title={t('selectRole.noneTitle')} detail={t('selectRole.noneDetail')} />
  }

  return (
    <section className="mx-auto flex w-full max-w-md flex-col gap-3.5" data-testid="select-role">
      <div className="flex flex-col gap-1">
        <h1 className="type-screen-title">{t('selectRole.title')}</h1>
        <p style={{ fontSize: 14, lineHeight: 1.5, color: 'var(--ink-2)', textWrap: 'pretty' }}>
          {t('selectRole.detail')}
        </p>
      </div>

      <ul className="flex flex-col gap-2.5">
        {memberships.map((m) => {
          const project = scope.data.projects[m.project_id] ?? m.project_id
          // village_id NULL is whole-project scope, which is ops and admin.
          // Falling back to the id keeps a row identifiable rather than blank.
          const village = m.village_id
            ? (scope.data.villages[m.village_id] ?? m.village_id)
            : t('selectRole.wholeProject')

          return (
            <li key={m.id}>
              <button
                type="button"
                data-testid={`select-role-${m.role}`}
                className="flex w-full flex-col justify-center gap-1 px-4 py-3 text-left hover:bg-primary-tint"
                style={{
                  minHeight: 64,
                  border: '1.5px solid var(--rule-2)',
                  borderRadius: 'var(--radius-control)',
                  background: 'var(--paper)',
                  color: 'var(--ink)',
                  fontFamily: 'inherit',
                }}
                onClick={() => void navigate({ to: roleHome(m.role), replace: true })}
              >
                <span style={{ fontSize: 16, fontWeight: 600 }}>{t(`role.${m.role}`)}</span>
                <span style={{ fontSize: 13, color: 'var(--ink-2)' }}>
                  {project} · {village}
                </span>
              </button>
            </li>
          )
        })}
      </ul>
    </section>
  )
}
