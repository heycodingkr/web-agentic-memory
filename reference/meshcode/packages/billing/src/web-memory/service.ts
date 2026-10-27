import { createHash, randomUUID } from "node:crypto"
import { z } from "zod"
import seed from "./seed.json"

export const tools = ["browser_navigate", "browser_click", "browser_type", "browser_eval", "browser_scroll",
  "browser_screenshot", "browser_snapshot", "browser_read", "browser_find_text", "browser_wait_for", "browser_script"] as const
const domain = z.string().trim().toLowerCase().max(253).regex(/^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/)
  .transform((value) => value.replace(/^(?:www\.)+/, ""))
const secret = /(?:\b(?:password|passwd|token|api[_-]?key|secret|authorization)\s*[:=]\s*\S{3,}|\bbearer\s+[\w.+/-]{16,}|\bsk-[\w-]{16,}|[\w.+-]+@[\w.-]+\.[a-z]{2,})/i
export const entry = z.object({
  id: z.string().regex(/^[a-z0-9][a-z0-9-]{0,63}$/), kind: z.enum(["workflow", "site_note", "gotcha"]),
  site: domain, title: z.string().min(1).max(120), summary: z.string().min(1).max(400),
  detail: z.string().max(8000), evidence: z.string().max(2000), tags: z.array(z.string().max(32)).max(8),
  status: z.literal("approved"), verified_at: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
}).strict().refine((value) => !secret.test([value.title, value.summary, value.detail, value.evidence, ...value.tags].join(" ")),
  "credentials or personal account data are forbidden")
export const entries = z.array(entry).max(32).refine((value) => new Set(value.map((item) => item.id)).size === value.length, "duplicate ids")
export const catalog = z.object({ schema_version: z.literal(1), revision: z.string().min(1).max(80), entries }).strict()
export const report = z.object({
  schema_version: z.literal(1), app_version: z.string().min(1).max(32).regex(/^[0-9A-Za-z.+_-]+$/), domain,
  terminal: z.enum(["completed", "failed", "cancelled"]),
  actions: z.array(z.object({ tool: z.enum(tools), outcome: z.enum(["ok", "error"]) }).strict()).min(1).max(24),
}).strict()
export type Catalog = z.infer<typeof catalog>
export type Report = z.infer<typeof report>
export type Candidate = {
  id: string; base_revision: string; entries: Catalog["entries"]; digest: string; created_by: string;
  created_at: string; verification?: { digest: string; suite: string; evidence: string; actor: string; at: string };
  published?: string
}
export type Release = { catalog: Catalog; actor: string; at: string; candidate?: string; rollback?: string }
export type State = { catalog: Catalog; candidates: Candidate[]; releases: Release[] }
export interface Repo {
  read(): Promise<State>
  update(fn: (state: State) => State): Promise<State>
}
export const initial = (): State => {
  const first = catalog.parse(seed)
  return { catalog: first, candidates: [], releases: [{ catalog: structuredClone(first), actor: "bundled", at: "2026-10-16T00:00:00.000Z" }] }
}
export const digest = (value: Catalog["entries"]) => createHash("sha256").update(JSON.stringify(value)).digest("hex")
export class Conflict extends Error {}
export function propose(state: State, base: string, items: Catalog["entries"], actor: string, at: string): State {
  if (state.catalog.revision !== base) throw new Conflict("stale base revision")
  const candidate = { id: randomUUID(), base_revision: base, entries: items, digest: digest(items), created_by: actor, created_at: at }
  return { ...state, candidates: [...state.candidates.slice(-9), candidate] }
}
export function release(state: State, items: Catalog["entries"], actor: string, at: string,
  origin: Pick<Release, "candidate" | "rollback">): State {
  const next = { schema_version: 1 as const, revision: randomUUID(), entries: items }
  return { ...state, catalog: next, releases: [...state.releases.slice(-9), { catalog: next, actor, at, ...origin }] }
}
export function memory(): Repo {
  let state = initial()
  return {
    read: async () => structuredClone(state),
    update: async (fn) => { state = fn(structuredClone(state)); return structuredClone(state) },
  }
}
