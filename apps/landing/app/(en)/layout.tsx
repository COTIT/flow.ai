import { localeMetadata, RootLayout } from "@/components/root-layout"

// One root layout per language (English at /); an unknown path matches none and gets the global 404.
export const generateMetadata = () => localeMetadata("en")

export default function EnglishLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return <RootLayout locale="en">{children}</RootLayout>
}
