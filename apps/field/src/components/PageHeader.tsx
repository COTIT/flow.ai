import { ArrowLeft } from 'lucide-react'
import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { BreadcrumbItem, BreadcrumbLink, Breadcrumbs } from '@/components/ui/breadcrumb'

export interface PageHeaderBreadcrumb {
  label: string
  to?: string
}

export function PageHeader({
  title,
  description,
  eyebrow,
  breadcrumbs,
  backTo,
  backSearch,
  backLabel,
  actions,
}: {
  title: ReactNode
  description?: ReactNode
  eyebrow?: ReactNode
  breadcrumbs?: PageHeaderBreadcrumb[]
  backTo?: string
  backSearch?: Record<string, string | undefined>
  backLabel?: string
  actions?: ReactNode
}) {
  const { t } = useTranslation()
  return (
    <header data-testid="page-header" className="flex w-full min-w-0 flex-col gap-4">
      {(backTo || breadcrumbs?.length) && (
        <div className="flex flex-col items-start gap-2 sm:flex-row sm:items-center sm:justify-between">
          {backTo && (
            <Link
              to={backTo as never}
              search={backSearch as never}
              className="inline-flex min-h-11 items-center gap-2 rounded-[var(--radius-control)] px-2 font-semibold text-primary-ink focus-visible:outline-2 focus-visible:outline-primary focus-visible:outline-offset-2"
            >
              <ArrowLeft aria-hidden size={16} strokeWidth={2.25} />
              {backLabel ?? t('common.back')}
            </Link>
          )}
          {breadcrumbs?.length ? (
            <Breadcrumbs className={backTo ? 'sm:ml-auto' : undefined}>
              {breadcrumbs.map((crumb, index) => (
                <BreadcrumbItem key={`${crumb.label}-${index}`} current={!crumb.to}>
                  {crumb.to ? <BreadcrumbLink href={crumb.to}>{crumb.label}</BreadcrumbLink> : crumb.label}
                </BreadcrumbItem>
              ))}
            </Breadcrumbs>
          ) : null}
        </div>
      )}

      <div className="flex min-w-0 flex-col gap-4 md:flex-row md:items-end md:justify-between">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          {eyebrow && <p className="type-section text-ink-3">{eyebrow}</p>}
          <h1 className="type-screen-title text-balance">{title}</h1>
          {description && <p className="type-body max-w-3xl text-ink-2">{description}</p>}
        </div>
        {actions && (
          <div
            data-testid="page-header-actions"
            className="flex w-full min-w-0 flex-col items-stretch gap-2 sm:flex-row sm:flex-wrap sm:items-center md:w-auto md:shrink-0 md:justify-end [&>*]:w-full sm:[&>*]:w-auto"
          >
            {actions}
          </div>
        )}
      </div>
    </header>
  )
}
