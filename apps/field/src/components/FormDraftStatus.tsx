import { Notice } from '@genesy/ui/components/notice'
import { useTranslation } from 'react-i18next'

import { UnsavedDraftBadge } from '@/components/UnsavedDraftBadge'

export function FormDraftStatus({ dirty, storageError }: { dirty: boolean; storageError: boolean }) {
  const { t } = useTranslation()
  return (
    <>
      {dirty && <UnsavedDraftBadge />}
      {storageError && (
        <div data-testid="draft-storage-error">
          <Notice variant="warning" title={t('draft.storageError')} />
        </div>
      )}
    </>
  )
}
