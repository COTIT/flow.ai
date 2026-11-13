"use client"

import { useState } from "react"
import { useRouter } from "next/navigation"
import { useFormatter, useTranslations } from "next-intl"
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"
import { useSession } from "@genesy/api-client/auth"
import { useUsersMe } from "@genesy/api-client/endpoints/users"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { useVoice } from "@genesy/voice/components/provider"
import { TranscriptRows } from "@genesy/voice/components/transcript"
import { VoiceRequestError, voiceErrorMessage } from "@genesy/voice/lib/errors"
import { requestFailureKey } from "@genesy/voice/lib/helpers"
import {
  historyRequest,
  type HistoryConversation,
  type HistoryMessages,
} from "@/lib/history"

type List = {
  conversations: HistoryConversation[]
  cursor?: string | null
  has_more?: boolean
}
type Detail = { conversation: HistoryConversation; sources: unknown[] }

export function History() {
  const workspaceId = useSession((s) => s.workspaceId)
  const user = useUsersMe().data?.data.user
  return (
    <HistoryBrowser
      key={`${user?.id}:${workspaceId}`}
      owner={`${user?.id}:${workspaceId}`}
      ready={Boolean(user?.id && workspaceId)}
    />
  )
}

function HistoryBrowser({ owner, ready }: { owner: string; ready: boolean }) {
  const t = useTranslations("history")
  const tv = useTranslations("voice")
  const format = useFormatter()
  const voice = useVoice()
  const router = useRouter()
  const client = useQueryClient()
  const [selected, setSelected] = useState<string | null>(null)
  const [search, setSearch] = useState("")
  const [q, setQ] = useState("")
  const [messageSearch, setMessageSearch] = useState("")
  const [messageQ, setMessageQ] = useState("")
  const [deleting, setDeleting] = useState(false)
  const base = ["conversation-history", owner]
  const enabled = ready && voice.historyEnabled
  const list = useInfiniteQuery({
    queryKey: [...base, "list", q],
    enabled,
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam, signal }) =>
      historyRequest<List>(
        { command: "list", q, cursor: pageParam, limit: 30 },
        signal
      ),
    getNextPageParam: (last) =>
      last.has_more && last.cursor ? last.cursor : undefined,
    retry: false,
  })
  const detail = useQuery({
    queryKey: [...base, "get", selected],
    enabled: enabled && Boolean(selected),
    queryFn: ({ signal }) =>
      historyRequest<Detail>(
        { command: "get", logicalConversationId: selected },
        signal
      ),
    retry: false,
  })
  const conversation = detail.data?.conversation
  const messages = useInfiniteQuery({
    queryKey: [...base, "messages", selected, conversation?.revision, messageQ],
    enabled: enabled && Boolean(conversation),
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam, signal }) =>
      historyRequest<HistoryMessages>(
        {
          command: "messages",
          logicalConversationId: selected,
          q: messageQ,
          revision: conversation?.revision,
          cursor: pageParam,
          limit: 100,
        },
        signal
      ),
    getNextPageParam: (last) =>
      last.has_more && last.cursor ? last.cursor : undefined,
    retry: false,
  })
  const action = useMutation({
    mutationFn: async (
      command: "new" | "text" | "voice" | "end" | "delete"
    ) => {
      if (command === "new") {
        await voice.startNewConversation(true)
        router.push("/")
        return
      }
      if (!selected) return
      if (command === "text" || command === "voice") {
        await voice.continueConversation(selected, command === "text")
        router.push("/")
        return
      }
      if (command === "delete")
        await voice.deleteConversation(selected)
      else if (command === "end" && selected === voice.logicalConversationId)
        await voice.endConversation()
      else await historyRequest({ command, logicalConversationId: selected })
      if (command === "delete") {
        setSelected(null)
        setDeleting(false)
      }
      await client.invalidateQueries({ queryKey: base })
    },
  })
  const error = list.error || detail.error || messages.error || action.error
  const errorText =
    error instanceof VoiceRequestError
      ? error.kind
        ? voiceErrorMessage(error, tv)
        : tv(requestFailureKey(error.status))
      : t("error")
  const rows = messages.data?.pages.flatMap((page) => page.messages) ?? []
  const partial =
    conversation &&
    (conversation.coverage !== "complete" ||
      messages.data?.pages.some((page) => page.gaps?.length))
  const busy = action.isPending || voice.historyRestoring
  const dates = (value: string) => {
    const date = new Date(value)
    return Number.isFinite(date.getTime())
      ? format.dateTime(date, { dateStyle: "medium" })
      : ""
  }
  return (
    <section
      data-history
      className="mx-auto flex w-full max-w-3xl flex-col gap-5 p-1 pb-8"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold tracking-tight">{t("title")}</h1>
        <Button
          className="min-h-11"
          disabled={busy || !enabled}
          onClick={() => action.mutate("new")}
        >
          {t("new")}
        </Button>
      </div>
      <p className="max-w-[65ch] text-sm text-muted-foreground">{t("intro")}</p>
      <form
        className="flex flex-wrap items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault()
          setQ(search.trim())
        }}
      >
        <label
          className="flex min-w-0 flex-1 flex-col gap-2 text-sm"
          htmlFor="history-search"
        >
          {t("searchLabel")}
          <Input
            id="history-search"
            className="min-h-11"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </label>
        <Button className="min-h-11" type="submit" disabled={!enabled}>
          {t("search")}
        </Button>
      </form>
      {error ? (
        <div
          role="alert"
          className="flex flex-wrap items-center gap-3 rounded-2xl border border-destructive p-4"
        >
          <p>{errorText}</p>
          <Button
            className="min-h-11"
            variant="outline"
            onClick={() => {
              action.reset()
              void client.invalidateQueries({ queryKey: base })
            }}
          >
            {t("retry")}
          </Button>
        </div>
      ) : null}
      {list.isPending ? <p role="status">{t("loading")}</p> : null}
      {list.data && !list.data.pages.some((p) => p.conversations.length) ? (
        <p role="status">{t(q ? "noResults" : "empty")}</p>
      ) : null}
      <ul className="flex flex-col gap-2" aria-label={t("title")}>
        {list.data?.pages
          .flatMap((page) => page.conversations)
          .map((item) => (
            <li key={item.id}>
              <button
                type="button"
                aria-pressed={selected === item.id}
                onClick={() => {
                  setSelected(item.id)
                  setMessageQ("")
                  setMessageSearch("")
                  action.reset()
                }}
                className="flex min-h-11 w-full flex-wrap items-center justify-between gap-2 rounded-2xl border border-(--hair) bg-card p-4 text-left focus-visible:outline-2 focus-visible:outline-(--gold) aria-pressed:border-(--gold)"
              >
                <span className="min-w-0 font-medium break-words">
                  {item.title || t("untitled")}
                </span>
                <span className="text-sm text-muted-foreground">
                  {t(`state.${item.state}`)} · {dates(item.last_activity_at)}
                </span>
              </button>
            </li>
          ))}
      </ul>
      {list.hasNextPage ? (
        <Button
          className="min-h-11 self-start"
          variant="outline"
          disabled={list.isFetchingNextPage}
          onClick={() => void list.fetchNextPage()}
        >
          {t("more")}
        </Button>
      ) : null}
      {selected && detail.isPending ? (
        <p role="status">{t("loading")}</p>
      ) : null}
      {conversation ? (
        <article
          className="flex flex-col gap-4 rounded-2xl border border-(--hair) bg-card p-4 md:p-6"
          aria-label={conversation.title || t("untitled")}
        >
          <h2 className="text-xl font-semibold break-words">
            {conversation.title || t("untitled")}
          </h2>
          <p className="text-sm text-muted-foreground">
            {t(`state.${conversation.state}`)} ·{" "}
            {t("expires", { date: dates(conversation.expires_at) })}
          </p>
          {partial ? (
            <p role="status" className="text-sm">
              {t("partial")}
            </p>
          ) : null}
          <div className="flex flex-wrap gap-2">
            {conversation.state === "active" ? (
              <>
                <Button
                  className="min-h-11"
                  disabled={busy}
                  onClick={() => action.mutate("text")}
                >
                  {t("continueText")}
                </Button>
                <Button
                  className="min-h-11"
                  variant="outline"
                  disabled={busy}
                  onClick={() => action.mutate("voice")}
                >
                  {t("continueVoice")}
                </Button>
                <Button
                  className="min-h-11"
                  variant="outline"
                  disabled={busy}
                  onClick={() => action.mutate("end")}
                >
                  {t("end")}
                </Button>
              </>
            ) : null}
            <Button
              className="min-h-11"
              variant="outline"
              disabled={busy}
              onClick={() => setDeleting(true)}
            >
              {t("delete")}
            </Button>
          </div>
          {conversation.state === "ended" ? (
            <p className="text-sm text-muted-foreground">{t("readOnly")}</p>
          ) : null}
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault()
              setMessageQ(messageSearch.trim())
            }}
          >
            <label
              className="flex min-w-0 flex-1 flex-col gap-2 text-sm"
              htmlFor="history-messages-search"
            >
              {t("messageSearch")}
              <Input
                id="history-messages-search"
                className="min-h-11"
                value={messageSearch}
                onChange={(e) => setMessageSearch(e.target.value)}
              />
            </label>
            <Button className="min-h-11" type="submit">
              {t("search")}
            </Button>
          </form>
          {messages.isPending ? (
            <p role="status">{t("loading")}</p>
          ) : !rows.length ? (
            <p role="status">{t(messageQ ? "noResults" : "noMessages")}</p>
          ) : null}
          <div className="flex flex-col gap-3" aria-label={t("transcript")}>
            {rows.map((row) => (
              <div key={row.id} className="flex flex-col gap-1">
                <TranscriptRows
                  rows={[{ id: row.id, role: row.role, text: row.text }]}
                />
                <p
                  className={`text-sm text-muted-foreground ${row.role === "user" ? "self-end" : "self-start"}`}
                >
                  {t(`modality.${row.modality}`)}
                  {row.status !== "complete"
                    ? ` · ${t(`status.${row.status}`)}`
                    : ""}
                  {row.provenance === "provisional"
                    ? ` · ${t("provisional")}`
                    : ""}
                  {row.interrupted ? ` · ${t("interrupted")}` : ""}
                </p>
              </div>
            ))}
          </div>
          {messages.hasNextPage ? (
            <Button
              className="min-h-11 self-start"
              variant="outline"
              disabled={messages.isFetchingNextPage}
              onClick={() => void messages.fetchNextPage()}
            >
              {t("moreMessages")}
            </Button>
          ) : null}
        </article>
      ) : null}
      <ConfirmDialog
        open={deleting}
        onOpenChange={setDeleting}
        title={t("delete")}
        description={t("deleteDescription")}
        confirm={t("confirmDelete")}
        cancel={t("cancel")}
        onConfirm={() => action.mutate("delete")}
      />
    </section>
  )
}
