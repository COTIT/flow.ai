import { Button } from "@genesy/ui/components/button"

export default function Page() {
  return (
    <main className="flex min-h-svh flex-col gap-3 p-6">
      <h1 className="text-xl font-medium">Genesy Shop</h1>
      <p className="text-muted-foreground text-sm">
        Public web shop, product pages and QR batch pages.
      </p>
      <Button className="w-fit">Shared button</Button>
    </main>
  )
}
