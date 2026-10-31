import { useMemo, useState } from 'react'
import { createColumnHelper } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'

import { useSession } from '@/app/session'
import { activeMemberships } from '@/app/membership'
import { DataTable } from '@/components/DataTable'
import { ErrorState } from '@/components/ErrorState'
import { CONTROL } from '@/components/controlStyles'
import { Loading } from '@/components/controls'
import { PageHeader } from '@/components/PageHeader'
import { TableSurface } from '@/components/TableSurface'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import { FormDraftStatus } from '@/components/FormDraftStatus'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { usePersistentForm } from '@/lib/usePersistentForm'
import {
  BUYER_CHANNELS,
  useBuyers,
  useCreateBuyer,
  type Buyer,
} from '@/features/ops/useOpsReference'

/**
 * Spec 7.5 — buyers. List and create.
 *
 * `channel = 'afm'` is a LABEL ONLY: no API, no integration, no partnership
 * assumed (Plan S12, and the migration says so at the table). The screen states
 * that rather than leaving a reader to infer a connection that does not exist.
 */
export function BuyersScreen() {
  const { t } = useTranslation()
  const query = useBuyers()
  const create = useCreateBuyer()
  const { data: session } = useSession()

  const [createOpen, setCreateOpen] = useState(false)

  // Ops and admin hold whole-project scope, so the membership names the
  // project a new buyer belongs to.
  const projectId = activeMemberships(session?.memberships ?? [])[0]?.project_id
  const draft = usePersistentForm('buyer-create', projectId ?? 'project', BUYER_DEFAULTS, zodResolver(buyerSchema))
  const [name, setName] = draft.field('name')
  const [channel, setChannel] = draft.field('channel')
  const [contactNote, setContactNote] = draft.field('contact_note')

  const columns = useMemo(() => {
    const col = createColumnHelper<Buyer>()
    return [
      col.accessor('name', { header: t('buyers.colName') }),
      col.accessor('channel', {
        header: t('buyers.colChannel'),
        cell: (c) => t(`buyers.channel.${c.getValue()}`),
      }),
      col.accessor('contact_note', {
        header: t('buyers.colContact'),
        cell: (c) => c.getValue() ?? '—',
      }),
      col.accessor('is_active', {
        header: t('buyers.colActive'),
        cell: (c) => (c.getValue() ? t('common.yes') : t('common.no')),
      }),
    ]
  }, [t])

  const submit = draft.handleSubmit(() => {
    if (!projectId || !draft.ready) return
    void finishDraftWhenSaved(create.mutateAsync({
      id: draft.clientRef,
      project_id: projectId,
      name: name.trim(),
      channel,
      contact_note: contactNote.trim() || null,
    }), draft.finish)
  })

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  return (
    <section className="flex flex-col gap-4">
      <PageHeader
        title={t('buyers.title')}
        description={<span data-testid="buyers-note">{t('buyers.channelNote')}</span>}
        actions={
          <Button
            type="button"
            data-testid="buyer-create-open"
            aria-expanded={createOpen}
            aria-controls="buyer-create-panel"
            variant={createOpen ? 'secondary' : 'primary'}
            onClick={() => setCreateOpen((open) => !open)}
          >
            {createOpen ? t('common.close') : t('buyers.create')}
          </Button>
        }
      />

      {query.isLoading ? (
        <Loading testId="buyers-loading" />
      ) : (
        <TableSurface>
          <DataTable
            columns={columns}
            data={query.data ?? []}
            testId="buyers-table"
            rowTestId="buyer-row"
            empty={{ title: t('buyers.noneTitle'), detail: t('buyers.noneDetail') }}
          />
        </TableSurface>
      )}

      {createOpen && (
        <section
          id="buyer-create-panel"
          data-testid="buyer-create-panel"
          className="flex w-full flex-col gap-3 p-4 sm:p-[18px]"
          style={{
            border: '1px solid var(--rule)',
            borderRadius: 'var(--radius-card)',
            background: 'var(--paper)',
          }}
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>{t('buyers.createTitle')}</h2>
            <Button
              type="button"
              data-testid="buyer-create-close"
              variant="ghost"
              size="sm"
              onClick={() => setCreateOpen(false)}
            >
              {t('common.close')}
            </Button>
          </div>

        <form onSubmit={submit} className="flex flex-col gap-3" noValidate>
        <FormDraftStatus dirty={draft.dirty} storageError={draft.storageError} />
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          <label className="flex min-w-0 flex-col gap-1.5" style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>
            <span className="block type-note" style={{ color: 'var(--ink-2)' }}>{t('buyers.colName')}</span>
            <input
              data-testid="buyer-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full" style={CONTROL}
            />
            {draft.formState.errors.name && (
              <span data-testid="buyer-name-error" className="block type-note font-medium" style={{ color: 'var(--flag-ink)' }}>
                {t('buyers.nameRequired')}
              </span>
            )}
          </label>

          <label className="flex min-w-0 flex-col gap-1.5" style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>
            <span className="block type-note" style={{ color: 'var(--ink-2)' }}>{t('buyers.colChannel')}</span>
            <Select value={channel} onValueChange={(value) => setChannel(value as Buyer['channel'])}>
              <SelectTrigger data-testid="buyer-channel" className="w-full">
                {t(`buyers.channel.${channel}`)}
              </SelectTrigger>
              <SelectContent>
                {BUYER_CHANNELS.map((c) => (
                  <SelectItem key={c} value={c}>
                    {t(`buyers.channel.${c}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </label>

          <label className="flex min-w-0 flex-col gap-1.5" style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>
            <span className="block type-note" style={{ color: 'var(--ink-2)' }}>{t('buyers.colContact')}</span>
            <input
              data-testid="buyer-contact-note"
              value={contactNote}
              onChange={(e) => setContactNote(e.target.value)}
              className="w-full" style={CONTROL}
            />
          </label>
        </div>

        {/* The database's message as written — a unique-name violation names
            the collision better than a generic failure would. */}
        {create.error && <ErrorState error={create.error} />}

        <Button
          type="submit"
          data-testid="buyer-create-submit"
          disabled={!draft.ready || create.isPending}
          className="w-fit"
        >
          {create.isPending ? t('buyers.creating') : t('buyers.create')}
        </Button>
        </form>
        </section>
      )}
    </section>
  )
}

const buyerSchema = z.object({
  name: z.string().trim().min(1),
  channel: z.enum(['direct', 'afm', 'other']),
  contact_note: z.string(),
})
const BUYER_DEFAULTS: z.infer<typeof buyerSchema> = { name: '', channel: 'direct', contact_note: '' }
