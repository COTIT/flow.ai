import { useState } from 'react'
import { getRouteApi } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useSession } from '@/app/session'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { ProvenanceBadge } from '@/components/ProvenanceBadge'
import { railColour } from '@/components/controlStyles'
import { Loading } from '@/components/controls'
import { VerificationMark } from '@/components/marks'
import { countUnverified, type PersonDetail, type Provenance } from '@/features/officer/personDetail'
import { usePersonDetail, useVerify } from '@/features/officer/usePersonDetail'
import { PlotDocuments } from '@/features/officer/PlotDocuments'
import { VerifyButton } from '@/features/officer/VerifyButton'
import { FarmerLoginCard } from '@/features/officer/FarmerLoginCard'
import { useLoginHistory, type LoginHistoryRow } from '@/features/officer/useFarmerLogin'
import { OfficerEditDialog, type EditField } from '@/features/officer/OfficerEditDialog'
import { useCrops } from '@/features/officer/useCrops'
import { useOfficerEdit } from '@/features/officer/useOfficerEdit'
import { selectedEditMeasure, type CropMeasure, type EditContext, type EditableTable, type EditValues } from '@/features/officer/officerEdit'
import { formatArea, formatKg, formatPlainDate, formatTimestamp } from '@/lib/format'

const route = getRouteApi('/_officer/officer/people/$personId')

/**
 * Spec 5.4 — person detail with provenance on every record, and the verify
 * action.
 *
 * Verifying invalidates the record key plus the farmer-facing and Tower keys,
 * so the badge flips here and on My Farm without a manual refresh.
 */
export function PersonDetailScreen() {
  const { personId } = route.useParams()
  const { t } = useTranslation()
  const query = usePersonDetail(personId)
  const verify = useVerify(personId, query.data?.person.village_id)
  const crops = useCrops()
  const userId = useSession().data?.userId
  const [editing, setEditing] = useState<EditState | null>(null)
  const edit = useOfficerEdit(editing?.measure)

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  if (query.isLoading) {
    return (
      <Loading testId="person-loading" />
    )
  }

  // Zero rows is an answer: RLS says this person is not visible here.
  if (!query.data) {
    return <EmptyState title={t('person.notFoundTitle')} detail={t('person.notFoundDetail')} />
  }

  const detail = query.data
  const outstanding = countUnverified(detail)
  const verifying = (table: string, id: string) =>
    verify.isPending && verify.variables?.table === table && verify.variables?.id === id

  const openEdit = (state: EditState) => {
    edit.reset()
    setEditing(state)
  }

  const saveEdit = (values: EditValues) => {
    /* c8 ignore next -- the dialog is only mounted while editing is non-null. */
    if (!editing) return
    // mutateAsync, so a failed save rejects: the dialog keeps its draft and
    // clears it only after the server confirmed.
    return edit.mutateAsync({
      table: editing.table,
      id: editing.id,
      values,
      context: editing.context,
      measure: selectedEditMeasure(editing.table, values, editing.measureByCrop, editing.measure),
    })
  }

  return (
    <section className="flex w-full flex-col gap-[18px]" data-testid="person-detail">
      <header className="flex flex-col gap-2.5">
        <div className="flex flex-wrap items-center gap-2.5">
          <h1 className="type-screen-title">
            {detail.person.given_name} {detail.person.family_name}
          </h1>
          <Badge record={detail.person} />
          <VerifyButton
            table="person"
            id={detail.person.id}
            recordLabel={`${detail.person.given_name} ${detail.person.family_name}`}
            verification={detail.person.verification}
            onVerify={verify.mutateAsync}
            pending={verifying('person', detail.person.id)}
          />
          <button type="button" data-testid="edit-person" onClick={() => openEdit(personEdit(detail.person))} style={EDIT_BUTTON}>
            {t('officerEdit.action')}
          </button>
        </div>
        <p
          data-testid="person-outstanding"
          className="inline-flex items-center gap-2.5"
          style={{ fontSize: 15, fontWeight: 500 }}
        >
          <VerificationMark verification={outstanding === 0 ? 'verified' : 'unverified'} size={18} />
          {outstanding === 0
            ? t('person.allVerified')
            : t('person.unverifiedCount', { count: outstanding })}
        </p>
        {detail.person.phone && (
          <p className="tabular" style={{ fontSize: 14, color: 'var(--ink-2)' }}>
            {detail.person.phone}
          </p>
        )}
      </header>

      {verify.isError && (
        <div data-testid="verify-error">
          <ErrorState error={verify.error} onRetry={() => verify.reset()} />
        </div>
      )}

      <Section title={t('person.households')}>
        {detail.households.length === 0 ? (
          <EmptyState title={t('person.noHouseholds')} />
        ) : (
          detail.households.map((h) => (
            <Card key={h.id} testId={`household-${h.id}`}>
              <Row
                title={h.label}
                record={h}
                edit={<button type="button" data-testid={`edit-household-${h.id}`} onClick={() => openEdit(householdEdit(h, detail.person.village_id, personId))} style={EDIT_BUTTON}>{t('officerEdit.action')}</button>}
                action={
                  // Four eyes on households (20260929090002_household_four_eyes):
                  // the officer who registered one may not verify it. app_verify
                  // refuses regardless; this spares a button that can only
                  // fail. The outstanding count above still includes it — it
                  // is outstanding, for somebody else.
                  h.verification !== 'verified' && userId !== undefined && h.captured_by === userId ? (
                    <p
                      data-testid={`verify-needs-second-staff-${h.id}`}
                      className="type-note max-w-56"
                      style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}
                    >
                      {t('verifyQueue.householdNeedsSecondStaff')}
                    </p>
                  ) : (
                    <VerifyButton
                      table="household"
                      id={h.id}
                      recordLabel={h.label}
                      verification={h.verification}
                      onVerify={verify.mutateAsync}
                      pending={verifying('household', h.id)}
                    />
                  )
                }
              />
              <p className="type-note" style={{ color: 'var(--ink-2)' }}>
                {t('person.members')}:{' '}
                {h.members.map((m) => `${m.given_name} ${m.family_name}`).join(', ')}
              </p>
            </Card>
          ))
        )}
      </Section>

      <Section title={t('person.farms')}>
        {detail.farms.length === 0 ? (
          <EmptyState title={t('person.noFarms')} detail={t('person.noFarmsDetail')} />
        ) : (
          detail.farms.map((farm) => (
            <Card key={farm.id} testId={`farm-${farm.id}`}>
              <Row
                title={farm.label}
                record={farm}
                edit={<button type="button" data-testid={`edit-farm-${farm.id}`} onClick={() => openEdit(farmEdit(farm, detail.person.village_id, personId))} style={EDIT_BUTTON}>{t('officerEdit.action')}</button>}
                action={
                  <VerifyButton
                    table="farm"
                    id={farm.id}
                    recordLabel={farm.label}
                    verification={farm.verification}
                    onVerify={verify.mutateAsync}
                    pending={verifying('farm', farm.id)}
                  />
                }
              />

              {farm.plots.map((plot) => (
                <div
                  key={plot.id}
                  data-testid={`plot-${plot.id}`}
                  className="ml-1 flex flex-col gap-2.5 pl-3.5"
                  style={{ borderLeft: `3px solid ${railColour(plot.verification)}` }}
                >
                  <Row
                    title={`${plot.label} · ${formatArea(plot.area_ha, 'hectare')}`}
                    record={plot}
                    edit={<button type="button" data-testid={`edit-plot-${plot.id}`} onClick={() => openEdit(plotEdit(plot, detail.person.village_id, personId, farm.id))} style={EDIT_BUTTON}>{t('officerEdit.action')}</button>}
                    action={
                      <VerifyButton
                        table="plot"
                        id={plot.id}
                        recordLabel={plot.label}
                        verification={plot.verification}
                      onVerify={verify.mutateAsync}
                        pending={verifying('plot', plot.id)}
                      />
                    }
                  />
                  <PlotDocuments plotId={plot.id} villageId={detail.person.village_id} />

                  {plot.cycles.map((cycle) => (
                    <div
                      key={cycle.id}
                      data-testid={`cycle-${cycle.id}`}
                      className="ml-1 flex flex-col gap-2.5 pl-3.5"
                      style={{ borderLeft: `3px solid ${railColour(cycle.verification)}` }}
                    >
                      <Row
                        title={cycle.season_label ? `${cycle.crop_name} · ${cycle.season_label}` : cycle.crop_name}
                        record={cycle}
                        edit={<button type="button" data-testid={`edit-cycle-${cycle.id}`} onClick={() => openEdit(cycleEdit(cycle, detail.person.village_id, personId, farm.id, crops.crops))} style={EDIT_BUTTON}>{t('officerEdit.action')}</button>}
                        action={
                          <VerifyButton
                            table="crop_cycle"
                            id={cycle.id}
                            recordLabel={cycle.crop_name}
                            verification={cycle.verification}
                          onVerify={verify.mutateAsync}
                            pending={verifying('crop_cycle', cycle.id)}
                          />
                        }
                      />
                      <p className="type-note tabular" style={{ color: 'var(--ink-3)' }}>
                        {t('person.window')}: {formatPlainDate(cycle.harvest_start)} –{' '}
                        {formatPlainDate(cycle.harvest_end)}
                      </p>

                      {cycle.harvests.map((h) => (
                        <div
                          key={h.id}
                          data-testid={`harvest-${h.id}`}
                          className="ml-1 pl-3.5"
                          style={{ borderLeft: `3px solid ${railColour(h.verification)}` }}
                        >
                              <Row
                            title={`${t(`person.${h.kind}`)} ${formatKg(h.quantity_kg)}${
                              h.is_current ? '' : ` (${t('person.superseded')})`
                            }`}
                                record={h}
                                edit={h.is_current ? <button type="button" data-testid={`edit-harvest-${h.id}`} onClick={() => openEdit(harvestEdit(h, detail.person.village_id, personId, farm.id, cycle.id))} style={EDIT_BUTTON}>{t('officerEdit.action')}</button> : undefined}
                                action={
                                  <VerifyButton
                                    table="harvest_report"
                                    id={h.id}
                                    recordLabel={`${h.kind} ${formatKg(h.quantity_kg)}`}
                                verification={h.verification}
                                onVerify={verify.mutateAsync}
                                pending={verifying('harvest_report', h.id)}
                              />
                            }
                          />
                        </div>
                      ))}
                    </div>
                  ))}
                </div>
              ))}
            </Card>
          ))
        )}
      </Section>
      {/* The route is officer-only (field officer, ops, admin), so every
          viewer here is staff; app_farmer_login_issue re-checks the village. */}
      <AppLoginSection personId={personId} />

      {editing && (
        <OfficerEditDialog
          table={editing.table}
          recordId={editing.id}
          version={editing.version}
          title={editing.title}
          fields={editing.fields}
          initialValues={editing.initialValues}
          measure={editing.measure}
          measureByCrop={editing.measureByCrop}
          pending={edit.isPending}
          error={edit.error}
          onSave={saveEdit}
          onComplete={() => setEditing(null)}
          onCancel={() => { edit.reset(); setEditing(null) }}
        />
      )}
    </section>
  )
}

/**
 * Whether this person can sign in to the app, who issued or reset that login,
 * and the action to create or reset it. Names and dates only — the temporary
 * password is shown once, inside the card, from the call that made it.
 */
function AppLoginSection({ personId }: { personId: string }) {
  const { t } = useTranslation()
  const history = useLoginHistory(personId)

  return (
    <Section title={t('farmerLogin.historyTitle')}>
      <Card testId="app-login">
        {history.data ? (
          <>
            <LoginHistory rows={history.data} />
            {/* Keyed by person: a credential shown for one farmer must never
                survive navigation to another. */}
            <FarmerLoginCard key={personId} personId={personId} hasLogin={history.data.length > 0} />
          </>
        ) : history.error ? (
          // A failed read is not "no login": offering Create here would reset
          // the password of a farmer who already has one.
          <ErrorState error={history.error} onRetry={() => void history.refetch()} />
        ) : (
          <Loading testId="login-history-loading" />
        )}
      </Card>
    </Section>
  )
}

function LoginHistory({ rows }: { rows: LoginHistoryRow[] }) {
  const { t } = useTranslation()

  if (rows.length === 0) {
    return (
      <p data-testid="login-history-none" className="type-body" style={{ color: 'var(--ink-2)' }}>
        {t('farmerLogin.historyNone')}
      </p>
    )
  }

  return (
    <div className="flex flex-col gap-2">
      {rows[0].must_change_password && (
        <p data-testid="login-must-change" className="type-body-strong" style={{ color: 'var(--primary-ink)' }}>
          {t('farmerLogin.mustChange')}
        </p>
      )}
      <ol data-testid="login-history" className="flex flex-col">
        {rows.map((row, index) => (
          <li
            key={`${row.issued_at}-${index}`}
            data-testid="login-history-row"
            data-kind={row.kind}
            className="flex flex-col gap-0.5 py-2 pl-3"
            style={{ borderLeft: '2px solid var(--rule-2)' }}
          >
            <span className="type-body-strong">
              {row.kind === 'reset'
                ? t('farmerLogin.historyReset', { name: row.issued_by_name })
                : t('farmerLogin.historyInitial', { name: row.issued_by_name })}
            </span>
            <span className="type-note tabular" style={{ color: 'var(--ink-3)' }}>
              {formatTimestamp(row.issued_at)}
            </span>
          </li>
        ))}
      </ol>
    </div>
  )
}

function Badge({ record }: { record: Provenance }) {
  return (
    <ProvenanceBadge
      source={record.source}
      verification={record.verification}
      confidence={record.confidence}
      capturedAt={record.captured_at}
    />
  )
}

/**
 * The mark leads, the name follows, the verify control sits at the end of the
 * row. Six levels of nesting mean this row is read dozens of times down one
 * screen, so provenance collapses to the compact variant here — the mark plus
 * one line of words — and the full lozenge is kept for the person at the top.
 */
function Row({
  title,
  record,
  edit,
  action,
}: {
  title: string
  record: Provenance
  edit?: React.ReactNode
  action?: React.ReactNode
}) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <VerificationMark verification={record.verification} size={18} />
      <span className="flex min-w-0 flex-1 flex-col gap-1">
        <b style={{ fontSize: 16, fontWeight: 600 }}>{title}</b>
        <ProvenanceBadge
          compact
          source={record.source}
          verification={record.verification}
          confidence={record.confidence}
          capturedAt={record.captured_at}
        />
      </span>
      {edit}
      {action}
    </div>
  )
}

interface EditState {
  table: EditableTable
  id: string
  title: string
  fields: EditField[]
  initialValues: EditValues
  context: EditContext
  measure?: 'area' | 'tree_count' | 'unit_count'
  measureByCrop?: Readonly<Record<string, CropMeasure>>
  /** The record's captured_at: a draft written against another version is discarded. */
  version: string | null
}

const harvestConfidenceField: EditField = {
  name: 'confidence',
  label: 'officerEdit.fields.confidence',
  options: [
    { value: 'low', label: 'confidence.low' },
    { value: 'medium', label: 'confidence.medium' },
    { value: 'high', label: 'confidence.high' },
  ],
}

const EDIT_BUTTON: React.CSSProperties = {
  minHeight: 40,
  border: '1px solid var(--rule-2)',
  borderRadius: 'var(--radius-control)',
  background: 'var(--paper)',
  padding: '0 12px',
  color: 'var(--primary-ink)',
  fontWeight: 600,
}

function personEdit(person: PersonDetail['person']): EditState {
  return {
    table: 'person', id: person.id, version: person.captured_at, title: 'officerEdit.titles.person',
    fields: [
      { name: 'given_name', label: 'officerEdit.fields.given_name', required: true },
      { name: 'family_name', label: 'officerEdit.fields.family_name', required: true },
      { name: 'phone', label: 'officerEdit.fields.phone' },
    ],
    initialValues: { given_name: person.given_name, family_name: person.family_name, phone: person.phone ?? '' },
    context: { villageId: person.village_id, personId: person.id },
  }
}

function householdEdit(household: PersonDetail['households'][number], villageId: string, personId: string): EditState {
  return {
    table: 'household', id: household.id, version: household.captured_at, title: 'officerEdit.titles.household',
    fields: [{ name: 'label', label: 'officerEdit.fields.household_label', required: true }],
    initialValues: { label: household.label },
    context: { villageId, personId },
  }
}

function farmEdit(farm: PersonDetail['farms'][number], villageId: string, personId: string): EditState {
  return {
    table: 'farm', id: farm.id, version: farm.captured_at, title: 'officerEdit.titles.farm',
    fields: [
      { name: 'label', label: 'officerEdit.fields.farm_label', required: true },
      { name: 'latitude', label: 'officerEdit.fields.latitude', type: 'number', step: 'any' },
      { name: 'longitude', label: 'officerEdit.fields.longitude', type: 'number', step: 'any' },
    ],
    initialValues: { label: farm.label, latitude: farm.latitude?.toString() ?? '', longitude: farm.longitude?.toString() ?? '' },
    context: { villageId, personId },
  }
}

function plotEdit(plot: PersonDetail['farms'][number]['plots'][number], villageId: string, personId: string, farmId: string): EditState {
  return {
    table: 'plot', id: plot.id, version: plot.captured_at, title: 'officerEdit.titles.plot',
    fields: [
      { name: 'label', label: 'officerEdit.fields.plot_label', required: true },
      { name: 'area_ha', label: 'officerEdit.fields.plot_area_ha', type: 'number', step: 'any' },
      { name: 'latitude', label: 'officerEdit.fields.latitude', type: 'number', step: 'any' },
      { name: 'longitude', label: 'officerEdit.fields.longitude', type: 'number', step: 'any' },
    ],
    initialValues: { label: plot.label, area_ha: plot.area_ha?.toString() ?? '', latitude: plot.latitude?.toString() ?? '', longitude: plot.longitude?.toString() ?? '' },
    context: { villageId, personId, farmId },
  }
}

function cycleEdit(
  cycle: PersonDetail['farms'][number]['plots'][number]['cycles'][number],
  villageId: string,
  personId: string,
  farmId: string,
  crops: Array<{ id: string; name: string; measured_by: 'area' | 'tree_count' | 'unit_count' }>,
): EditState {
  const measure = crops.find((crop) => crop.id === cycle.crop_id)?.measured_by
  return {
    table: 'crop_cycle', id: cycle.id, version: cycle.captured_at, title: 'officerEdit.titles.crop_cycle', measure,
    fields: [
      { name: 'crop_id', label: 'officerEdit.fields.crop_id', options: crops.map((crop) => ({ value: crop.id, label: crop.name })) },
      { name: 'season_label', label: 'officerEdit.fields.season_label' },
      { name: 'area_ha', label: 'officerEdit.fields.cycle_area_ha', type: 'number' as const, step: 'any' },
      { name: 'tree_count', label: 'officerEdit.fields.tree_count', type: 'number' as const, step: '1' },
      { name: 'unit_count', label: 'officerEdit.fields.unit_count', type: 'number' as const, step: '1' },
      { name: 'planted_on', label: 'officerEdit.fields.planted_on', type: 'date' },
      { name: 'harvest_start', label: 'officerEdit.fields.harvest_start', type: 'date' },
      { name: 'harvest_end', label: 'officerEdit.fields.harvest_end', type: 'date' },
      { name: 'status', label: 'officerEdit.fields.status', options: ['planned', 'growing', 'harvested', 'abandoned'].map((value) => ({ value, label: `cycleStatus.${value}` })) },
    ],
    initialValues: { crop_id: cycle.crop_id, season_label: cycle.season_label ?? '', area_ha: cycle.area_ha?.toString() ?? '', tree_count: cycle.tree_count?.toString() ?? '', unit_count: cycle.unit_count?.toString() ?? '', planted_on: cycle.planted_on ?? '', harvest_start: cycle.harvest_start ?? '', harvest_end: cycle.harvest_end ?? '', status: cycle.status },
    context: { villageId, personId, farmId, cycleId: cycle.id },
    measureByCrop: Object.fromEntries(crops.map((crop) => [crop.id, crop.measured_by])),
  }
}

function harvestEdit(
  harvest: PersonDetail['farms'][number]['plots'][number]['cycles'][number]['harvests'][number],
  villageId: string,
  personId: string,
  farmId: string,
  cycleId: string,
): EditState {
  return {
    table: 'harvest_report', id: harvest.id, version: harvest.captured_at, title: 'officerEdit.titles.harvest_report',
    fields: [
      { name: 'quantity_kg', label: 'officerEdit.fields.quantity_kg', type: 'number', step: 'any', required: true },
      { name: 'reported_for', label: 'officerEdit.fields.reported_for', type: 'date' },
      harvestConfidenceField,
    ],
    initialValues: { quantity_kg: harvest.quantity_kg.toString(), reported_for: harvest.reported_for ?? '', confidence: harvest.confidence ?? 'medium' },
    context: { villageId, personId, farmId, cycleId, harvestKind: harvest.kind },
  }
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-2.5">
      <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
        {title}
      </h2>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  )
}

function Card({ children, testId }: { children: React.ReactNode; testId: string }) {
  return (
    <div
      data-testid={testId}
      className="flex flex-col gap-3 p-4"
      style={{
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-card)',
        background: 'var(--paper)',
      }}
    >
      {children}
    </div>
  )
}
