import type { Metadata, Viewport } from "next"
import { NextIntlClientProvider } from "next-intl"
import { getLocale } from "next-intl/server"
import { headers } from "next/headers"

import "@genesy/ui/globals.css"
import "./office.css"
import "@genesy/voice/styles/voice.css"
import { QueryProvider } from "@genesy/api-client/query-provider"
import { ServiceWorker } from "@genesy/ui/components/service-worker"
import { fontSans } from "@genesy/ui/lib/fonts"
import { pwaMetadata, pwaViewport } from "@genesy/ui/lib/pwa"
import { cn } from "@genesy/ui/lib/utils"
import { ThemeProvider } from "@/components/theme-provider"

export const metadata: Metadata = {
  title: "Genesy Office",
  ...pwaMetadata("Office"),
}

export const viewport: Viewport = pwaViewport

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
      <body className="theme-glass">
        <NextIntlClientProvider>
          <ThemeProvider nonce={nonce}>
            <QueryProvider>{children}</QueryProvider>
          </ThemeProvider>
        </NextIntlClientProvider>
        <ServiceWorker />
      </body>
    </html>
  )
}
