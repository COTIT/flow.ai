"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"

import { useRights } from "@genesy/api-client/rights"
import { Button } from "@genesy/ui/components/button"
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@genesy/ui/components/tabs"
import {
  FLOW_CAPABILITIES,
  statusOf,
  type FlowGroup,
} from "@genesy/voice/lib/capabilities"
import { resetActivity } from "@/lib/activity"
import {
  activityVersion,
  dockAsk,
  useActivityKey,
  useRunCapability,
} from "@/lib/pills"
import { CapabilityCard } from "./capability-card"

const GROUPS: FlowGroup[] = ["understand", "operate", "automate", "create"]

/** "Explore what Genesy can do": the registry by kind of task, with this role's real status. */
export function Explorer() {
  const t = useTranslations("discovery.explorer")
  const rights = useRights()
  const key = useActivityKey()
  const [reset, setReset] = useState(false)
  // Try it on an instruction is sent through the dock, as a suggestion is.
  const run = useRunCapability((text) => dockAsk.getState().ask?.(text))
  const list = FLOW_CAPABILITIES.filter((c) => c.id !== "explore")
  return (
    <div className="flex flex-col gap-4 py-2">
      <div className="flex flex-col gap-1">
        <h1 className="text-2xl font-semibold text-balance">{t("title")}</h1>
        <p className="text-sm text-muted-foreground">{t("text")}</p>
      </div>
      <Tabs defaultValue="understand" className="min-w-0">
        <TabsList className="max-w-full overflow-x-auto">
          {GROUPS.map((group) => (
            <TabsTrigger key={group} value={group} className="min-h-11">
              {t(`groups.${group}`)}
            </TabsTrigger>
          ))}
        </TabsList>
        {GROUPS.map((group) => (
          <TabsContent key={group} value={group}>
            <ul className="grid gap-3 sm:grid-cols-2">
              {list
                .filter((c) => c.group === group)
                .map((cap) => (
                  <CapabilityCard
                    key={cap.id}
                    cap={cap}
                    status={
                      rights.read ? statusOf(cap, rights.may) : "restricted"
                    }
                    onTry={() => run(cap)}
                  />
                ))}
            </ul>
          </TabsContent>
        ))}
      </Tabs>
      <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
        <p className="mr-auto" role="status">
          {reset ? t("resetDone") : t("resetText")}
        </p>
        <Button
          variant="outline"
          className="min-h-11"
          disabled={!key}
          onClick={() => {
            if (!key) return
            resetActivity(key)
            activityVersion.getState().bump()
            setReset(true)
          }}
        >
          {t("reset")}
        </Button>
      </div>
    </div>
  )
}
