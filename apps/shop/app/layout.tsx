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
import { ShopProvider } from "@/components/shop-provider"
import { getShop, getShopBase } from "@/lib/shop-scope"

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations("app")
  const shop = await getShop()
  const title = shop?.name ?? t("title")
  return {
    // shortcut: localhost default for dev; every deploy must set SITE_URL.
    metadataBase: new URL(process.env.SITE_URL ?? "http://localhost:3000"),
    title: { default: title, template: `%s · ${title}` },
    description: shop ? undefined : t("tagline"),
    openGraph: { siteName: title, type: "website" },
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
  const base = await getShopBase()
  const shop = await getShop()
  return (
    <html
      lang={locale}
      suppressHydrationWarning
      className={cn("font-sans antialiased", fontSans.variable)}
    >
      <body className="theme-glass has-[[data-sticky-buy]]:max-md:pb-24">
        <NextIntlClientProvider>
          <ThemeProvider nonce={nonce}>
            <QueryProvider>
              <ShopProvider key={base} base={base} name={shop?.name ?? ""}>
                {/* Mock `ShopCatalogue` .wrap-shop: header, page and footer in one 1200 px column. */}
                <div className="mx-auto flex min-h-svh w-full max-w-[1200px] flex-col gap-5 px-3 pt-3 pb-8 text-[14px] leading-normal md:gap-7 md:px-6 md:pt-5 md:pb-12">
                  <SiteHeader shopName={shop?.name} />
                  <main className="flex flex-1 flex-col gap-5 md:gap-7">
                    {children}
                  </main>
                  <SiteFooter shopName={shop?.name} />
                </div>
              </ShopProvider>
            </QueryProvider>
          </ThemeProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  )
}
