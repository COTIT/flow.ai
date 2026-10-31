"use client"

import * as React from "react"
import { cn } from "cn"

import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"

export type LoginFormLabels = {
  title: string
  email: string
  password: string
  submit: string
}

/** Email + password form. The app supplies translated labels and does the login call. */
function LoginForm({
  labels,
  onSubmit,
  pending = false,
  error,
  className,
}: {
  labels: LoginFormLabels
  onSubmit: (credentials: { email: string; password: string }) => void
  pending?: boolean
  error?: string | null
  className?: string
}) {
  const errorId = React.useId()

  return (
    <form
      className={cn("flex w-full max-w-sm flex-col gap-4", className)}
      aria-describedby={error ? errorId : undefined}
      onSubmit={(e) => {
        e.preventDefault()
        const form = new FormData(e.currentTarget)
        onSubmit({
          email: String(form.get("email")),
          password: String(form.get("password")),
        })
      }}
    >
      <h1 className="text-xl font-medium">{labels.title}</h1>
      <div className="flex flex-col gap-2">
        <Label htmlFor="login-email">{labels.email}</Label>
        <Input
          id="login-email"
          name="email"
          type="email"
          autoComplete="email"
          required
          aria-invalid={Boolean(error)}
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label htmlFor="login-password">{labels.password}</Label>
        <Input
          id="login-password"
          name="password"
          type="password"
          autoComplete="current-password"
          maxLength={255}
          required
          aria-invalid={Boolean(error)}
        />
      </div>
      {error ? (
        <p id={errorId} role="alert" className="text-sm text-destructive">
          {error}
        </p>
      ) : null}
      <Button type="submit" disabled={pending}>
        {labels.submit}
      </Button>
    </form>
  )
}

export { LoginForm }
