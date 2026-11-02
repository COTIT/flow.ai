"use client"

import { MoonIcon, SunIcon } from "lucide-react"
import { useTheme } from "next-themes"

import { Button } from "@genesy/ui/components/button"

/**
 * shortcut: till copy of @genesy/ui ThemeToggle at the till's 44 px; delete it and pass
 * className="size-11" to ThemeToggle once its className prop is on main.
 */
export function ThemeButton({ label }: { label: string }) {
  const { resolvedTheme, setTheme } = useTheme()
  return (
    <Button
      variant="ghost"
      size="icon"
      className="size-11"
      aria-label={label}
      onClick={() => setTheme(resolvedTheme === "dark" ? "light" : "dark")}
    >
      <SunIcon className="dark:hidden" />
      <MoonIcon className="hidden dark:block" />
    </Button>
  )
}
