import type { PrismaClient } from "@prisma/client"
import { catalog, initial, type Repo, type State } from "./service"

// Existing SystemConfig storage: no schema migration. A transaction lock serializes all replicas.
const key = "web_agentic_memory_v1"
function decode(value: string | undefined): State {
  if (!value) return initial()
  const state = JSON.parse(value) as State
  catalog.parse(state.catalog)
  if (!Array.isArray(state.candidates) || !Array.isArray(state.releases)) throw new Error("invalid web memory state")
  return state
}
export function prisma(db: PrismaClient): Repo {
  return {
    read: async () => decode((await db.systemConfig.findUnique({ where: { key } }))?.value),
    update: (fn) => db.$transaction(async (tx) => {
      await tx.$queryRaw`SELECT pg_advisory_xact_lock(73271401)::text`
      const state = fn(decode((await tx.systemConfig.findUnique({ where: { key } }))?.value))
      const value = JSON.stringify(state)
      await tx.systemConfig.upsert({ where: { key }, create: { key, value }, update: { value } })
      return state
    }, { timeout: 10_000 }),
  }
}
