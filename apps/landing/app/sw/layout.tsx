import { localeMetadata, RootLayout } from "@/components/root-layout"

// One root layout per language (Swahili at /sw); an unknown path matches none and gets the global 404.
export const generateMetadata = () => localeMetadata("sw")

export default function SwahiliLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return <RootLayout locale="sw">{children}</RootLayout>
}
