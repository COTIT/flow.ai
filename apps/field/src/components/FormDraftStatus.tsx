import { useTranslation } from 'react-i18next'
import { UnsavedDraftBadge } from '@/components/UnsavedDraftBadge'

export function FormDraftStatus({ dirty, storageError }: { dirty: boolean; storageError: boolean }) {
  const { t } = useTranslation()
  return <>{dirty && <UnsavedDraftBadge />}{storageError && <p role="alert">{t('draft.storageError')}</p>}</>
}
