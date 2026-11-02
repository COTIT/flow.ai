import type { Metadata } from "next"
import { NextIntlClientProvider } from "next-intl"
import { getLocale } from "next-intl/server"
import { headers } from "next/headers"

import "@genesy/ui/globals.css"
import "./till.css"
import { QueryProvider } from "@genesy/api-client/query-provider"
import { fontSans } from "@genesy/ui/lib/fonts"
import { cn } from "@genesy/ui/lib/utils"
import { ThemeProvider } from "@/components/theme-provider"
import { TillProvider } from "@/components/till-provider"

export const metadata: Metadata = {
  title: "Genesy Till",
}

export default async function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  const locale = await getLocale()
  // Set by proxy.ts; next-themes' inline script needs it to pass the CSP.
  const nonce = (await headers()).get("x-nonce") ?? undefined
  return (
    <html
      lang={locale}
      suppressHydrationWarning
      className={cn("font-sans antialiased", fontSans.variable)}
    >
      <body className="theme-glass till-app">
        <NextIntlClientProvider>
          <ThemeProvider nonce={nonce}>
            <QueryProvider>
              <TillProvider>{children}</TillProvider>
            </QueryProvider>
          </ThemeProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  )
}
