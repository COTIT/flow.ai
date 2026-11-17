"use client"

import { useTranslations } from "next-intl"

import { PageHead } from "@/components/list"
import { NoAccess } from "@/components/states"
import { useCan, useRights } from "@/lib/can"

import { JournalForm } from "../journal-form"

/** A new manual journal draft (Form card template). */
export default function NewJournalPage() {
  const t = useTranslations()
  const rights = useRights()
  const may = useCan("accounting.journal_create")
  if (rights.read && !may) return <NoAccess />
  return (
    <>
      <PageHead
        back={{
          href: "/accounting/journals",
          label: t("accounting.journals.title"),
        }}
        eyebrow={t("shell.accounting")}
        title={t("accounting.journals.newTitle")}
      />
      <JournalForm />
    </>
  )
}
