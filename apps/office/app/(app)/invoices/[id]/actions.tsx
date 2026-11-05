"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  useInvoicePaymentsCreate,
  useInvoicePaymentsMarkAsCredited,
} from "@genesy/api-client/endpoints/invoice-payments"
import {
  useInvoicesCredit,
  useInvoicesDelete,
  useInvoicesDuplicate,
  useInvoicesList,
  useInvoicesResend,
  useInvoicesSend,
  useInvoicesUpdate,
} from "@genesy/api-client/endpoints/invoices"
import type { InvoiceAction, InvoiceResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { Textarea } from "@genesy/ui/components/textarea"
import { decimalToMinor, toMinor } from "@genesy/ui/lib/money"
import { Tile } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { Done, useDone } from "@/components/states"
import { useRights } from "@/lib/can"
import { useIdempotencyKey } from "@/lib/idempotency"

import { invoiceMoney, invoiceNumber } from "../shared"

// The rights each action's route checks (docs/api/invoices.md, invoice-payments.md).
const rights: Record<InvoiceAction, string[]> = {
  edit: ["invoice.update"],
  send: ["invoice.send", "workspace.send-message"],
  resend: ["invoice.send", "workspace.send-message"],
  cancel: ["invoice.delete"],
  duplicate: ["invoice.create"],
  credit: ["invoice.credit", "invoice.create"],
  pay: ["invoicepayment.create"],
  mark_as_credited: ["invoice.credit"],
}
// The big button goes to the first of these the invoice allows; undoing ones never get it.
const mainOrder: InvoiceAction[] = [
  "send",
  "pay",
  "mark_as_credited",
  "credit",
  "resend",
]
const restOrder: InvoiceAction[] = [
  "edit",
  "send",
  "pay",
  "credit",
  "mark_as_credited",
  "resend",
  "cancel",
  "duplicate",
]

type Contact = {
  id: string
  full_name?: string | null
  email?: string | null
  default_contact?: boolean
}
type Relation = {
  id?: string
  addresses?: Record<string, unknown>[]
  relationships?: { contacts?: Contact[] }
}
const ADDRESS_KEYS = [
  "address",
  "house_number",
  "house_number_addition",
  "address2",
  "zipcode",
  "city",
  "state",
  "country",
]

/** 503 = the server has no PDF renderer, file store or mail set up (staging). */
const unavailable = (e: unknown) =>
  (e as { response?: { status?: number } } | null)?.response?.status === 503

const today = () => new Date().toISOString().slice(0, 10)

/** Minor units as the plain amount the form shows and sends: 2100, 2 → "21.00". */
function plain(minor: bigint, dp: number) {
  const neg = minor < 0n
  const digits = (neg ? -minor : minor).toString().padStart(dp + 1, "0")
  const whole = digits.slice(0, digits.length - dp)
  return `${neg ? "-" : ""}${whole}${dp ? `.${digits.slice(-dp)}` : ""}`
}

/** OfficeOrder's Next step card for an invoice: only the API's `allowed_actions` show. */
export function NextStep({
  inv,
  onChanged,
}: {
  inv: InvoiceResource
  onChanged: () => void
}) {
  const t = useTranslations()
  const router = useRouter()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const { may } = useRights()
  const done = useDone()
  const [dialog, setDialog] = React.useState<InvoiceAction | null>(null)
  const vars = { workspace: workspaceId, invoice: inv.id }
  const ok = (message: string) => () => {
    setDialog(null)
    done.show(message)
    onChanged()
  }
  const cancel = useInvoicesDelete({
    mutation: {
      onMutate: done.clear,
      onSuccess: ok(t("done.invoice.cancelled")),
    },
  })
  const duplicate = useInvoicesDuplicate({
    mutation: {
      onMutate: done.clear,
      onSuccess: ({ data }) => router.push(`/invoices/${data.invoice_id}`),
    },
  })

  const allowed = inv.allowed_actions.filter((a) => rights[a].every(may))
  const main = mainOrder.find((a) => allowed.includes(a))
  const rest = restOrder.filter((a) => a !== main && allowed.includes(a))
  const button = (a: InvoiceAction, big: boolean) => (
    <Button
      key={a}
      {...(big
        ? ({ size: "lg", className: "w-full" } as const)
        : ({
            variant: "outline",
            className: a === "cancel" ? "w-full text-(--tone-error)" : "w-full",
          } as const))}
      disabled={
        (a === "duplicate" && duplicate.isPending) ||
        (a === "cancel" && cancel.isPending)
      }
      onClick={() =>
        a === "duplicate" ? duplicate.mutate(vars) : setDialog(a)
      }
    >
      {t(`invoices.act.${a}`)}
    </Button>
  )
  const close = (open: boolean) => {
    if (!open) setDialog(null)
  }

  return (
    <Tile id="invoice-next" title={t("invoices.nextStep")} span="col-span-4">
      <Done message={done.message} />
      {inv.status === "draft" ? (
        <Notice variant="info" title={t("invoices.draftInfo")} />
      ) : null}
      {inv.allowed_actions.length === 0 ? (
        <Notice title={t("invoices.nothingToDo")} />
      ) : null}
      {main ? (
        <>
          <p className="text-[13px]">{t(`invoices.next.${main}`)}</p>
          {button(main, true)}
        </>
      ) : null}
      {rest.length ? (
        <div className="grid grid-cols-2 gap-2 max-md:grid-cols-1">
          {rest.map((a) => button(a, false))}
        </div>
      ) : null}
      <ApiError
        error={cancel.error ?? duplicate.error}
        fallback={t("common.error")}
      />

      {dialog === "send" || dialog === "resend" ? (
        <SendDialog
          inv={inv}
          resend={dialog === "resend"}
          onOpenChange={close}
          onDone={ok(
            t(dialog === "send" ? "done.invoice.sent" : "done.invoice.resent")
          )}
        />
      ) : null}
      {dialog === "pay" ? (
        <PayDialog
          inv={inv}
          onOpenChange={close}
          onDone={ok(t("done.invoice.paid"))}
        />
      ) : null}
      {dialog === "credit" ? (
        <CreditDialog inv={inv} onOpenChange={close} />
      ) : null}
      {dialog === "mark_as_credited" ? (
        <MarkDialog
          inv={inv}
          onOpenChange={close}
          onDone={ok(t("done.invoice.marked"))}
        />
      ) : null}
      {dialog === "edit" ? (
        <EditDialog
          inv={inv}
          onOpenChange={close}
          onDone={ok(t("done.invoice.saved"))}
        />
      ) : null}
      <ConfirmDialog
        open={dialog === "cancel"}
        onOpenChange={close}
        title={t("invoices.cancelTitle")}
        description={t("invoices.cancelText")}
        confirm={t("invoices.act.cancel")}
        cancel={t("invoices.keep")}
        disabled={cancel.isPending}
        onConfirm={() => cancel.mutate(vars)}
      />
    </Tile>
  )
}

/** Shared frame of the action dialogs: open while mounted, closed through `onOpenChange`. */
function ActionDialog({
  title,
  description,
  onOpenChange,
  children,
}: {
  title: string
  description?: string
  onOpenChange: (open: boolean) => void
  children: React.ReactNode
}) {
  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {description ? (
            <DialogDescription>{description}</DialogDescription>
          ) : null}
        </DialogHeader>
        {children}
      </DialogContent>
    </Dialog>
  )
}

function Footer({ submit, disabled }: { submit: string; disabled: boolean }) {
  const t = useTranslations()
  return (
    <DialogFooter>
      <DialogClose render={<Button type="button" variant="outline" />}>
        {t("actions.cancel")}
      </DialogClose>
      <Button type="submit" disabled={disabled}>
        {submit}
      </Button>
    </DialogFooter>
  )
}

/** A radio with a 44 px row to tap. */
function Radio({
  name,
  checked,
  onChange,
  label,
}: {
  name: string
  checked: boolean
  onChange: () => void
  label: string
}) {
  return (
    <label className="flex min-h-11 items-center gap-2 text-sm">
      <input
        type="radio"
        name={name}
        checked={checked}
        onChange={onChange}
        // ::after widens the 16 px dot to a 44 px hit area.
        className="relative size-4 shrink-0 appearance-none rounded-full border border-(--field-border) bg-(--field) after:absolute after:-inset-3.5 after:content-[''] checked:border-[5px] checked:border-primary"
      />
      {label}
    </label>
  )
}

function SendDialog({
  inv,
  resend,
  onOpenChange,
  onDone,
}: {
  inv: InvoiceResource
  resend: boolean
  onOpenChange: (open: boolean) => void
  onDone: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const relation = (inv.relationships.relation ?? {}) as Relation
  const contacts = relation.relationships?.contacts ?? []
  const first = contacts.find((c) => c.default_contact) ?? contacts[0]
  const [contactId, setContactId] = React.useState(first?.id ?? "")
  const [email, setEmail] = React.useState(resend)
  const [to, setTo] = React.useState(first?.email ?? "")
  // The invoice's own address snapshot, else the customer's billing (or first) address.
  const source =
    (inv.billing_address as Record<string, unknown> | null) ??
    relation.addresses?.find((a) => a.type === "billing") ??
    relation.addresses?.[0]
  const address = source
    ? Object.fromEntries(
        ADDRESS_KEYS.filter((k) => source[k] != null).map((k) => [k, source[k]])
      )
    : null
  const opts = { mutation: { onSuccess: onDone } }
  const send = useInvoicesSend(opts)
  const again = useInvoicesResend(opts)
  const call = resend ? again : send
  const errors = fieldErrors(call.error)

  return (
    <ActionDialog
      title={t(resend ? "invoices.resendTitle" : "invoices.sendTitle")}
      onOpenChange={onOpenChange}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const form = new FormData(e.currentTarget)
          const data = {
            addressed_contact_id: contactId,
            address: address ?? {},
            send_method: email ? "email" : "manual",
            ...(email
              ? {
                  to: [to],
                  subject: String(form.get("subject") ?? ""),
                  body: String(form.get("body") ?? ""),
                }
              : {}),
          }
          call.mutate({ workspace: workspaceId, invoice: inv.id, data })
        }}
      >
        {contacts.length === 0 ? (
          <Notice variant="warning" title={t("invoices.noContact")} />
        ) : null}
        {!address ? (
          <Notice variant="warning" title={t("invoices.noAddress")} />
        ) : null}
        {contacts.length > 1 ? (
          <Field
            id="send-contact"
            label={t("invoices.sendContact")}
            errors={errors.addressed_contact_id}
          >
            <select
              className={selectClass}
              value={contactId}
              onChange={(e) => setContactId(e.target.value)}
            >
              {contacts.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.full_name || c.email}
                </option>
              ))}
            </select>
          </Field>
        ) : null}
        {resend ? null : (
          <fieldset className="flex flex-col">
            <legend className="mb-1 text-sm font-medium">
              {t("invoices.sendHow")}
            </legend>
            <Radio
              name="how"
              checked={!email}
              onChange={() => setEmail(false)}
              label={t("invoices.sendManual")}
            />
            <Radio
              name="how"
              checked={email}
              onChange={() => setEmail(true)}
              label={t("invoices.sendEmail")}
            />
          </fieldset>
        )}
        {email ? (
          <>
            <Field id="send-to" label={t("invoices.sendTo")} errors={errors.to}>
              <Input
                type="email"
                value={to}
                onChange={(e) => setTo(e.target.value)}
                required
              />
            </Field>
            <Field
              id="send-subject"
              label={t("invoices.sendSubject")}
              errors={errors.subject}
            >
              <Input
                name="subject"
                defaultValue={t("invoices.sendSubjectDefault", {
                  nr: "{invoice.invoice_nr}",
                })}
                required
              />
            </Field>
            <Field
              id="send-body"
              label={t("invoices.sendMessage")}
              errors={errors.body}
            >
              <Textarea
                name="body"
                defaultValue={t("invoices.sendMessageDefault")}
                required
              />
            </Field>
          </>
        ) : null}
        {unavailable(call.error) ? (
          <Notice variant="warning" title={t("invoices.emailUnavailable")} />
        ) : (
          <ApiError error={call.error} fallback={t("common.error")} />
        )}
        <Footer
          submit={t(resend ? "invoices.act.resend" : "invoices.act.send")}
          disabled={call.isPending || !contactId || !address}
        />
      </form>
    </ActionDialog>
  )
}

function PayDialog({
  inv,
  onOpenChange,
  onDone,
}: {
  inv: InvoiceResource
  onOpenChange: (open: boolean) => void
  onDone: () => void
}) {
  const t = useTranslations()
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const currency = inv.relationships.currency
  const dp = currency?.decimals ?? 2
  const open = decimalToMinor(String(inv.balance.open), dp)
  const [amount, setAmount] = React.useState(
    open !== null && open > 0n ? plain(open, dp) : ""
  )
  const [method, setMethod] = React.useState("cash")
  const [note, setNote] = React.useState("")
  const key = useIdempotencyKey()
  const pay = useInvoicePaymentsCreate({
    mutation: { onSuccess: onDone, onSettled: (_d, e) => key.settle(e) },
    request: key.request,
  })
  const relation = (inv.relationships.relation ?? {}) as Relation
  const notes = useInvoicesList(
    workspaceId,
    { relation_id: relation.id, is_credit: true, status: "pending" },
    { query: { enabled: method === "settle" && Boolean(relation.id) } }
  )
  const minor = toMinor(amount, dp)
  const errors = fieldErrors(pay.error)

  return (
    <ActionDialog title={t("invoices.payTitle")} onOpenChange={onOpenChange}>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          if (minor === null) return
          const form = new FormData(e.currentTarget)
          pay.mutate({
            workspace: workspaceId,
            invoice: inv.id,
            data: {
              date: String(form.get("date")),
              amount: plain(BigInt(minor), dp),
              ...(method === "settle"
                ? { settle_invoice_id: String(form.get("note_id")) }
                : { payment_method: method }),
              ...(note ? { description: note } : {}),
            },
          })
        }}
      >
        <Field
          id="pay-amount"
          label={t("invoices.payAmount", {
            currency: currency?.asset_type ?? "",
          })}
          errors={
            amount && minor === null ? [t("invoices.badAmount")] : errors.amount
          }
        >
          <Input
            inputMode="decimal"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            required
          />
        </Field>
        <Field id="pay-date" label={t("invoices.payDate")} errors={errors.date}>
          <Input name="date" type="date" defaultValue={today()} required />
        </Field>
        <Field
          id="pay-method"
          label={t("invoices.payMethod")}
          errors={errors.payment_method}
        >
          <select
            className={selectClass}
            value={method}
            onChange={(e) => setMethod(e.target.value)}
          >
            {(["cash", "pin", "manual", "settle"] as const).map((m) => (
              <option key={m} value={m}>
                {t(`invoices.payBy.${m}`)}
              </option>
            ))}
          </select>
        </Field>
        {method === "settle" ? (
          notes.data && notes.data.data.length === 0 ? (
            <Notice variant="warning" title={t("invoices.noCreditNotes")} />
          ) : (
            <Field
              id="pay-note-id"
              label={t("invoices.payCreditNote")}
              errors={errors.settle_invoice_id}
            >
              <select name="note_id" className={selectClass} required>
                {(notes.data?.data ?? []).map((n) => (
                  <option key={n.id} value={n.id}>
                    {t("invoices.creditNoteOption", {
                      number: invoiceNumber(n.invoice_nr) ?? "—",
                      open: invoiceMoney(
                        n.balance.open,
                        n.relationships.currency,
                        locale
                      ),
                    })}
                  </option>
                ))}
              </select>
            </Field>
          )
        ) : null}
        <Field
          id="pay-description"
          label={t("invoices.payNote")}
          errors={errors.description}
        >
          <Input
            value={note}
            onChange={(e) => setNote(e.target.value)}
            maxLength={180}
          />
        </Field>
        <ApiError error={pay.error} fallback={t("common.error")} />
        <Footer
          submit={t("invoices.act.pay")}
          disabled={
            pay.isPending ||
            minor === null ||
            (method === "settle" && !notes.data?.data.length)
          }
        />
      </form>
    </ActionDialog>
  )
}

function CreditDialog({
  inv,
  onOpenChange,
}: {
  inv: InvoiceResource
  onOpenChange: (open: boolean) => void
}) {
  const t = useTranslations()
  const router = useRouter()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [all, setAll] = React.useState(true)
  const [qty, setQty] = React.useState<Record<string, string>>({})
  const credit = useInvoicesCredit({
    mutation: {
      onSuccess: ({ data }) => router.push(`/invoices/${data.invoice_id}`),
    },
  })
  const errors = fieldErrors(credit.error)
  const chosen = inv.items.flatMap((item) =>
    Number(qty[item.id]) > 0
      ? [{ id: item.id, credit_amount: Number(qty[item.id]) }]
      : []
  )

  return (
    <ActionDialog
      title={t("invoices.creditDialogTitle")}
      description={t("invoices.creditDialogText")}
      onOpenChange={onOpenChange}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          credit.mutate({
            workspace: workspaceId,
            invoice: inv.id,
            data: all ? { credit_all: true } : { items: chosen },
          })
        }}
      >
        <fieldset className="flex flex-col">
          <Radio
            name="what"
            checked={all}
            onChange={() => setAll(true)}
            label={t("invoices.creditAll")}
          />
          <Radio
            name="what"
            checked={!all}
            onChange={() => setAll(false)}
            label={t("invoices.creditLines")}
          />
        </fieldset>
        {all
          ? null
          : inv.items.map((item) => (
              <div key={item.id} className="flex flex-col gap-1">
                <Field
                  id={`credit-${item.id}`}
                  label={t("invoices.creditQty", { title: item.title })}
                  // The API names a row by its place in `items`, which holds the chosen lines only.
                  errors={
                    errors[
                      `items.${chosen.findIndex((c) => c.id === item.id)}.credit_amount`
                    ]
                  }
                >
                  <Input
                    inputMode="decimal"
                    value={qty[item.id] ?? ""}
                    onChange={(e) =>
                      setQty({ ...qty, [item.id]: e.target.value })
                    }
                  />
                </Field>
                <p className="text-xs text-muted-foreground">
                  {t("invoices.creditLeft", {
                    left: String(
                      (item as { amount_left_to_credit?: string })
                        .amount_left_to_credit ?? item.quantity
                    ),
                  })}
                </p>
              </div>
            ))}
        <ApiError error={credit.error} fallback={t("common.error")} />
        <Footer
          submit={t("invoices.makeCredit")}
          disabled={credit.isPending || (!all && chosen.length === 0)}
        />
      </form>
    </ActionDialog>
  )
}

function MarkDialog({
  inv,
  onOpenChange,
  onDone,
}: {
  inv: InvoiceResource
  onOpenChange: (open: boolean) => void
  onDone: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const key = useIdempotencyKey()
  const mark = useInvoicePaymentsMarkAsCredited({
    mutation: { onSuccess: onDone, onSettled: (_d, e) => key.settle(e) },
    request: key.request,
  })
  const errors = fieldErrors(mark.error)
  return (
    <ActionDialog
      title={t("invoices.markTitle")}
      description={t("invoices.markText")}
      onOpenChange={onOpenChange}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const form = new FormData(e.currentTarget)
          mark.mutate({
            workspace: workspaceId,
            invoice: inv.id,
            data: {
              date: String(form.get("date")),
              payment_method: String(form.get("method")),
            },
          })
        }}
      >
        <Field
          id="mark-date"
          label={t("invoices.payDate")}
          errors={errors.date}
        >
          <Input name="date" type="date" defaultValue={today()} required />
        </Field>
        <Field
          id="mark-method"
          label={t("invoices.payMethod")}
          errors={errors.payment_method}
        >
          <select name="method" className={selectClass} defaultValue="manual">
            {(["cash", "pin", "manual"] as const).map((m) => (
              <option key={m} value={m}>
                {t(`invoices.payBy.${m}`)}
              </option>
            ))}
          </select>
        </Field>
        <ApiError error={mark.error} fallback={t("common.error")} />
        <Footer
          submit={t("invoices.act.mark_as_credited")}
          disabled={mark.isPending}
        />
      </form>
    </ActionDialog>
  )
}

function EditDialog({
  inv,
  onOpenChange,
  onDone,
}: {
  inv: InvoiceResource
  onOpenChange: (open: boolean) => void
  onDone: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const update = useInvoicesUpdate({ mutation: { onSuccess: onDone } })
  const errors = fieldErrors(update.error)
  const relation = (inv.relationships.relation ?? {}) as Relation
  return (
    <ActionDialog title={t("invoices.editTitle")} onOpenChange={onOpenChange}>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const form = new FormData(e.currentTarget)
          const text = (k: string) => String(form.get(k) ?? "").trim() || null
          update.mutate({
            workspace: workspaceId,
            invoice: inv.id,
            // shortcut: no line editor yet; lines left out stay as they are (API diff rules).
            data: {
              relation_id: relation.id ?? "",
              reference: text("reference"),
              note: text("note"),
              due_date: text("due_date"),
            },
          })
        }}
      >
        <Field
          id="edit-reference"
          label={t("invoices.reference")}
          errors={errors.reference}
        >
          <Input
            name="reference"
            defaultValue={inv.reference ?? ""}
            maxLength={255}
          />
        </Field>
        <Field id="edit-note" label={t("invoices.note")} errors={errors.note}>
          <Textarea
            name="note"
            defaultValue={inv.note ?? ""}
            maxLength={1000}
          />
        </Field>
        <Field
          id="edit-due"
          label={t("invoices.dueDate")}
          errors={errors.due_date}
        >
          <Input
            name="due_date"
            type="date"
            defaultValue={inv.due_date?.slice(0, 10) ?? ""}
          />
        </Field>
        <ApiError error={update.error} fallback={t("common.error")} />
        <Footer submit={t("invoices.save")} disabled={update.isPending} />
      </form>
    </ActionDialog>
  )
}
