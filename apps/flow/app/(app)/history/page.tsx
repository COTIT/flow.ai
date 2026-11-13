import { connection } from "next/server"
import { notFound } from "next/navigation"
import { History } from "./history"

export default async function HistoryPage() {
  await connection()
  if (process.env.ASSISTANT_CONVERSATIONS_ENABLED !== "true") notFound()
  return <History />
}
