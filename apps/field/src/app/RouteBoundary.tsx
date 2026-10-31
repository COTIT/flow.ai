import { Link, type ErrorComponentProps } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { ErrorState } from '@/components/ErrorState'
import { EmptyState } from '@/components/EmptyState'

export function RouteError({ error, reset }: ErrorComponentProps) {
  return <ErrorState error={error} onRetry={reset} />
}

export function RouteNotFound() {
  const { t } = useTranslation()
  return (
    <EmptyState
      title={t('notFound.title')}
      detail={t('notFound.detail')}
      action={
        <Link to="/" className="text-sm font-medium text-primary underline underline-offset-4">
          {t('notFound.home')}
        </Link>
      }
    />
  )
}
