import type { Metadata } from "next"
import { Geist, Geist_Mono } from "next/font/google"

import "@genesy/ui/globals.css"
import { QueryProvider } from "@genesy/api-client/query-provider"
import { cn } from "@genesy/ui/lib/utils"
import { ThemeProvider } from "@/components/theme-provider"

const fontSans = Geist({ subsets: ["latin"], variable: "--font-sans" })
const fontMono = Geist_Mono({ subsets: ["latin"], variable: "--font-mono" })

export const metadata: Metadata = {
  title: "Genesy Field",
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html
      lang="en"
      suppressHydrationWarning
      className={cn("antialiased font-sans", fontSans.variable, fontMono.variable)}
    >
      <body>
        <ThemeProvider>
          <QueryProvider>{children}</QueryProvider>
        </ThemeProvider>
      </body>
    </html>
  )
}
