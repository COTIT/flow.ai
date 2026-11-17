"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useTaxProfilesGet } from "@genesy/api-client/endpoints/tax-profiles"
import { DetailSkeleton } from "@/components/bento"
import { PageHead } from "@/components/list"
import { LoadError, NoAccess } from "@/components/states"
import { useCan, useRights } from "@/lib/can"

import { TaxForm } from "../tax-form"

/** A new tax profile draft, or (`?corrects=`) the next revision of a verified one. */
export default function NewTaxPage({
  searchParams,
}: {
  searchParams: Promise<{ corrects?: string }>
}) {
  const t = useTranslations()
  const rights = useRights()
  const may = useCan("taxprofile.configure")
  const { corrects } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const source = useTaxProfilesGet(workspaceId, corrects ?? "", {
    query: { enabled: Boolean(workspaceId && corrects) },
  })
  if (rights.read && !may) return <NoAccess />
  return (
    <>
      <PageHead
        back={{ href: "/settings/taxes", label: t("taxes.title") }}
        eyebrow={t("shell.settings")}
        title={corrects ? t("taxes.correctTitle") : t("taxes.newTitle")}
        description={corrects ? t("taxes.correctText") : undefined}
      />
      {!corrects ? (
        <TaxForm />
      ) : source.isPending ? (
        <DetailSkeleton />
      ) : source.isError ? (
        <LoadError
          error={source.error}
          onRetry={() => void source.refetch()}
          backHref="/settings/taxes"
        />
      ) : (
        <TaxForm corrects={source.data.data} />
      )}
    </>
  )
}
