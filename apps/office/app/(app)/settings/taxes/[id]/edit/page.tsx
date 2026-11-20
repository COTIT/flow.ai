"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import { useTaxProfilesGet } from "@verzo/api-client/endpoints/tax-profiles"
import { Notice } from "@verzo/ui/components/notice"
import { DetailSkeleton } from "@/components/bento"
import { PageHead } from "@/components/list"
import { LoadError } from "@/components/states"

import { TaxForm } from "../../tax-form"

/** Change a draft tax profile; a verified one says so and links back. */
export default function EditTaxPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const profile = useTaxProfilesGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId) },
  })
  if (profile.isPending) return <DetailSkeleton />
  if (profile.isError)
    return (
      <LoadError
        error={profile.error}
        onRetry={() => void profile.refetch()}
        backHref="/settings/taxes"
      />
    )
  const p = profile.data.data
  return (
    <>
      <PageHead
        back={{ href: `/settings/taxes/${p.id}`, label: t("taxes.title") }}
        eyebrow={t("shell.settings")}
        title={t("taxes.editTitle")}
      />
      {p.allowed_actions.includes("update") ? (
        <TaxForm profile={p} />
      ) : (
        <Notice
          variant="info"
          title={t("taxes.onlyDrafts")}
          action={
            <Link
              className="inline-flex items-center text-sm font-medium underline-offset-4 hover:underline max-md:min-h-11"
              href={`/settings/taxes/${p.id}`}
            >
              {t("taxes.backToProfile")}
            </Link>
          }
        />
      )}
    </>
  )
}
