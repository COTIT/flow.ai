import * as React from "react"

import { Label } from "@genesy/ui/components/label"

/** Label, control and the API's 422 messages for one form field. */
export function Field({
  id,
  label,
  errors,
  children,
}: {
  id: string
  label: string
  errors?: string[]
  children: React.ReactElement<{ id?: string }>
}) {
  const errorId = `${id}-error`
  return (
    <div className="flex flex-col gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {React.cloneElement(
        children as React.ReactElement<Record<string, unknown>>,
        {
          id,
          "aria-invalid": errors?.length ? true : undefined,
          "aria-describedby": errors?.length ? errorId : undefined,
        }
      )}
      {errors?.length ? (
        <p id={errorId} className="text-sm text-destructive">
          {errors.join(" ")}
        </p>
      ) : null}
    </div>
  )
}

/** Native `<select>` styled like the glass inputs (glass.css `.input.select`, 44 px). */
export const selectClass =
  "h-11 rounded-xl border border-(--field-border) bg-(--field) px-3.5 text-sm aria-invalid:border-destructive"
