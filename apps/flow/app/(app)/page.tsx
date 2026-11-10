"use client"

import { useQuery } from "@tanstack/react-query"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useUsersMe } from "@genesy/api-client/endpoints/users"
import { sessionStore } from "@genesy/api-client/session"
import { Button } from "@genesy/ui/components/button"
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@genesy/ui/components/card"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice, type NoticeVariant } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { useWorkspaceChoices } from "@/components/shell"
import { readinessState, type Readiness } from "@/lib/readiness"

const look: Record<
  Readiness,
  { variant: NoticeVariant; title: string; text?: string }
> = {
  available: { variant: "success", title: "assistantAvailable" },
  off: { variant: "info", title: "assistantOff", text: "assistantOffText" },
  not_ready: {
    variant: "warning",
    title: "assistantNotReady",
    text: "assistantNotReadyText",
  },
  unknown: { variant: "error", title: "assistantUnknown" },
}

/** Who is signed in, which workspace, and whether Office's assistant answers. */
export default function Home() {
  const t = useTranslations("flow")
  const me = useUsersMe()
  const workspaceId = useSession((s) => s.workspaceId)
  const { query: workspaces, choices } = useWorkspaceChoices()
  const current = choices.find((w) => w.id === workspaceId)
  const readiness = useQuery({
    queryKey: ["voice-readiness"],
    retry: false,
    queryFn: async () => {
      const response = await fetch("/api/voice/readiness", {
        cache: "no-store",
      })
      const body: unknown = await response.json().catch(() => undefined)
      return readinessState(response.status, body)
    },
  })
  const state: Readiness | undefined = readiness.isError
    ? "unknown"
    : readiness.data
  const user = me.data?.data.user

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>
            <h1>{t("assistant")}</h1>
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {state ? (
            <Notice
              variant={look[state].variant}
              title={t(look[state].title)}
              action={
                state === "unknown" || state === "not_ready" ? (
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={readiness.isFetching}
                    onClick={() => void readiness.refetch()}
                  >
                    {t("retry")}
                  </Button>
                ) : null
              }
            >
              {look[state].text ? t(look[state].text) : null}
            </Notice>
          ) : (
            <Skeleton className="h-14 w-full" />
          )}
          <p className="text-sm text-muted-foreground">
            {t("conversationLater")}
          </p>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="grid gap-4 sm:grid-cols-2">
          <div className="min-w-0">
            <p className="text-xs text-muted-foreground">{t("signedInAs")}</p>
            {user ? (
              <>
                <p className="truncate font-medium">{user.name}</p>
                <p className="truncate text-sm text-muted-foreground">
                  {user.email}
                </p>
              </>
            ) : me.isError ? null : (
              <Skeleton className="mt-1 h-10 w-40" />
            )}
          </div>
          <div className="min-w-0">
            <p className="text-xs text-muted-foreground">{t("workspace")}</p>
            {current ? (
              <p className="truncate font-medium">{current.display_name}</p>
            ) : workspaces.isPending ? (
              <Skeleton className="mt-1 h-5 w-32" />
            ) : null}
          </div>
        </CardContent>
      </Card>

      {me.isError || workspaces.isError ? (
        <Notice
          variant="error"
          title={t("error")}
          action={
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                void me.refetch()
                void workspaces.refetch()
              }}
            >
              {t("retry")}
            </Button>
          }
        />
      ) : null}

      {workspaces.isSuccess && !choices.length ? (
        <Card>
          <CardContent>
            <EmptyState
              title={t("noWorkspace")}
              description={t("noWorkspaceText")}
            />
          </CardContent>
        </Card>
      ) : null}

      {workspaces.isSuccess && choices.length && !current ? (
        <Card>
          <CardHeader>
            <CardTitle>{t("chooseWorkspace")}</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-2">
            <p className="text-sm text-muted-foreground">
              {t("chooseWorkspaceText")}
            </p>
            {choices.map((w) => (
              <Button
                key={w.id}
                variant="outline"
                className="justify-start"
                onClick={() => sessionStore.getState().selectWorkspace(w.id)}
              >
                {w.display_name}
              </Button>
            ))}
          </CardContent>
        </Card>
      ) : null}
    </>
  )
}
