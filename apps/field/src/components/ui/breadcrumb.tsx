import type { ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'

export function Breadcrumbs({ children, className }: { children: ReactNode; className?: string }) {
  const { t } = useTranslation()
  return (
    <nav aria-label={t('a11y.breadcrumb')} className={cn('type-note text-ink-2', className)}>
      <ol className="flex flex-wrap items-center gap-1.5">{children}</ol>
    </nav>
  )
}

export function BreadcrumbItem({
  children,
  current = false,
}: {
  children: ReactNode
  current?: boolean
}) {
  return (
    <li aria-current={current ? 'page' : undefined} className="inline-flex items-center gap-1.5">
      {children}
      {current ? null : <span aria-hidden className="text-ink-3">/</span>}
    </li>
  )
}

export function BreadcrumbLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <Link to={href as never} className="font-medium text-primary-ink underline-offset-2 hover:underline">
      {children}
    </Link>
  )
}
