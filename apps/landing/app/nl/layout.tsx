import { localeMetadata, RootLayout } from "@/components/root-layout"

// One root layout per language (Dutch at /nl); an unknown path matches none and gets the global 404.
export const generateMetadata = () => localeMetadata("nl")

export default function DutchLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return <RootLayout locale="nl">{children}</RootLayout>
}
