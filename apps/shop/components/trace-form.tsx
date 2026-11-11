import { getTranslations } from "next-intl/server"
import { QrCodeIcon } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { cn } from "@genesy/ui/lib/utils"

import { goldChip, tile } from "@/lib/look"
import { getShopBase } from "@/lib/shop-scope"
import { shopPath } from "@/lib/shop-path"

/** Code under the QR -> its passport. A plain GET form: works before any script loads. */
export async function TraceForm({
  value,
  invalid,
  className,
}: {
  value?: string
  invalid?: boolean
  className?: string
}) {
  const t = await getTranslations("trace")
  return (
    <form
      action={shopPath(await getShopBase(), "/b")}
      method="get"
      className={cn(tile, "flex flex-col gap-3 p-5", className)}
    >
      <div className="flex items-center gap-3">
        <span className={cn(goldChip, "size-11 rounded-[14px]")}>
          <QrCodeIcon aria-hidden className="size-5" />
        </span>
        <p className="text-[16px] font-semibold">{t("title")}</p>
      </div>
      <p id="trace-help" className="text-[13px] text-muted-foreground">
        {t("body")}
      </p>
      <label htmlFor="trace-code" className="text-[13px] font-medium">
        {t("label")}
      </label>
      <div className="flex flex-wrap gap-2">
        <Input
          id="trace-code"
          name="code"
          defaultValue={value}
          required
          maxLength={20}
          autoComplete="off"
          autoCapitalize="characters"
          spellCheck={false}
          aria-describedby="trace-help"
          aria-invalid={invalid || undefined}
          className="h-11 min-w-0 flex-[1_1_160px] font-mono tracking-[0.12em] uppercase"
        />
        <Button type="submit" size="lg" className="h-11">
          {t("go")}
        </Button>
      </div>
    </form>
  )
}
