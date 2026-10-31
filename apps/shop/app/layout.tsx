import type { Metadata } from "next"
import { NextIntlClientProvider } from "next-intl"
import { getLocale, getTranslations } from "next-intl/server"
import { headers } from "next/headers"

import "@genesy/ui/globals.css"
import { QueryProvider } from "@genesy/api-client/query-provider"
import { fontSans } from "@genesy/ui/lib/fonts"
import { cn } from "@genesy/ui/lib/utils"
import { SiteFooter } from "@/components/site-footer"
import { SiteHeader } from "@/components/site-header"
import { ThemeProvider } from "@/components/theme-provider"

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations("app")
  return {
    // shortcut: localhost default for dev; every deploy must set SITE_URL.
    metadataBase: new URL(process.env.SITE_URL ?? "http://localhost:3000"),
    title: { default: t("title"), template: `%s · ${t("title")}` },
    description: t("tagline"),
    openGraph: { siteName: t("title"), type: "website" },
  }
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
      <body>
        <NextIntlClientProvider>
          <ThemeProvider nonce={nonce}>
            <QueryProvider>
              <div className="flex min-h-svh flex-col">
                <SiteHeader />
                <main className="mx-auto w-full max-w-6xl flex-1 px-4 py-6 md:px-6 md:py-10">
                  {children}
                </main>
                <SiteFooter />
              </div>
            </QueryProvider>
          </ThemeProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  )
}
