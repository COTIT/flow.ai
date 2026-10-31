"use client"

import * as React from "react"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@genesy/ui/components/alert-dialog"
import { Button } from "@genesy/ui/components/button"

/**
 * A button that asks before an irreversible action. `onConfirm` runs only
 * from the dialog's confirm button; cancel, Escape or the backdrop send nothing.
 */
function ConfirmDialog({
  trigger,
  title,
  description,
  confirm,
  cancel,
  onConfirm,
  disabled,
  destructive = true,
  open,
  onOpenChange,
}: {
  trigger?: string
  title: string
  description: string
  confirm: string
  cancel: string
  onConfirm: () => void
  disabled?: boolean
  destructive?: boolean
  /** Controlled use (no trigger): the caller opens it. */
  open?: boolean
  onOpenChange?: (open: boolean) => void
}) {
  const [inner, setInner] = React.useState(false)
  const isOpen = open ?? inner
  const setOpen = onOpenChange ?? setInner
  return (
    <AlertDialog open={isOpen} onOpenChange={setOpen}>
      {trigger && (
        <AlertDialogTrigger
          disabled={disabled}
          render={<Button variant={destructive ? "destructive" : "default"} />}
        >
          {trigger}
        </AlertDialogTrigger>
      )}
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>{cancel}</AlertDialogCancel>
          <AlertDialogAction
            variant={destructive ? "destructive" : "default"}
            onClick={() => {
              onConfirm()
              setOpen(false)
            }}
          >
            {confirm}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

export { ConfirmDialog }
