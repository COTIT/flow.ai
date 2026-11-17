"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useAccountingJournalsGet } from "@genesy/api-client/endpoints/accounting"
import { Notice } from "@genesy/ui/components/notice"
import { DetailSkeleton } from "@/components/bento"
import { PageHead } from "@/components/list"
import { LoadError } from "@/components/states"

import { JournalForm } from "../../journal-form"

/** Change a draft (full replace); a posted journal says so and links back. */
export default function EditJournalPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const journal = useAccountingJournalsGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId) },
  })
  if (journal.isPending) return <DetailSkeleton />
  if (journal.isError)
    return (
      <LoadError
        error={journal.error}
        onRetry={() => void journal.refetch()}
        backHref="/accounting/journals"
      />
    )
  const j = journal.data.data
  return (
    <>
      <PageHead
        back={{
          href: `/accounting/journals/${j.id}`,
          label: t("accounting.journals.draftTitle"),
        }}
        eyebrow={t("shell.accounting")}
        title={t("accounting.journals.editTitle")}
      />
      {j.allowed_actions.includes("update") ? (
        <JournalForm journal={j} />
      ) : (
        <Notice
          variant="info"
          title={t("accounting.journals.onlyDrafts")}
          action={
            <Link
              className="inline-flex items-center text-sm font-medium underline-offset-4 hover:underline max-md:min-h-11"
              href={`/accounting/journals/${j.id}`}
            >
              {t("accounting.journals.backToJournal")}
            </Link>
          }
        />
      )}
    </>
  )
}
