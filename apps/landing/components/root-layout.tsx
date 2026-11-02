import { NextIntlClientProvider } from "next-intl"
import { setRequestLocale } from "next-intl/server"
import { headers } from "next/headers"

import "@genesy/ui/globals.css"
import type { Locale } from "@genesy/i18n/locales"
import { fontSans } from "@genesy/ui/lib/fonts"
import { cn } from "@genesy/ui/lib/utils"
import { ThemeProvider } from "@/components/theme-provider"

/** The <html> shell both root layouts share: `/` (English) and `/nl`, `/sw`. */
export async function RootLayout({
  locale,
  children,
}: {
  locale: Locale
  children: React.ReactNode
}) {
  setRequestLocale(locale)
  // Set by proxy.ts; next-themes' inline script needs it to pass the CSP.
  const nonce = (await headers()).get("x-nonce") ?? undefined
  return (
    <html
      lang={locale}
      suppressHydrationWarning
      className={cn("font-sans antialiased", fontSans.variable)}
    >
      <body>
        <NextIntlClientProvider>
          <ThemeProvider nonce={nonce}>{children}</ThemeProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  )
}
