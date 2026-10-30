import { Button } from "@genesy/ui/components/button"

export default function Page() {
  return (
    <main className="flex min-h-svh flex-col gap-3 p-6">
      <h1 className="text-xl font-medium">Genesy Till</h1>
      <p className="text-muted-foreground text-sm">
        Point of sale and cash payouts.
      </p>
      <Button className="w-fit">Shared button</Button>
    </main>
  )
}
