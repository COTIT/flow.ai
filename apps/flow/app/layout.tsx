import type { Metadata, Viewport } from "next"
import { NextIntlClientProvider } from "next-intl"
import { getLocale } from "next-intl/server"
import { headers } from "next/headers"

import "@verzo/ui/globals.css"
import "@verzo/voice/styles/voice.css"
import { QueryProvider } from "@verzo/api-client/query-provider"
import { fontSans } from "@verzo/ui/lib/fonts"
import { cn } from "@verzo/ui/lib/utils"
import { OfflineNavigation } from "@/components/offline-navigation"
import { ThemeProvider } from "@/components/theme-provider"

export const metadata: Metadata = { title: "Verzo Flow" }

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  viewportFit: "cover",
}

export default async function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  const locale = await getLocale()
  // Set by proxy.ts; next-themes' inline script needs it to pass the CSP.
  const nonce = (await headers()).get("x-nonce") ?? undefined
  return (
    <html
      lang={locale}
      suppressHydrationWarning
      className={cn("font-sans antialiased", fontSans.variable)}
    >
      <body className="theme-glass">
        <NextIntlClientProvider>
          <ThemeProvider nonce={nonce}>
            <QueryProvider>{children}</QueryProvider>
            <OfflineNavigation />
          </ThemeProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  )
}
