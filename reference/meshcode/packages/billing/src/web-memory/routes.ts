import type { Context, Hono } from "hono"
import { bodyLimit } from "hono/body-limit"
import { z } from "zod"
import { authenticate, requireAdmin, AuthError } from "../auth/guard"
import { createThrottle } from "../auth/throttle"
import { clientIp } from "../http/request"
import { catalog, entries, report, digest, propose, release, Conflict, type Repo, type Report } from "./service"

export type Deps = { repo: Repo; submit: (report: Report) => Promise<void>; reports: () => Promise<unknown[]> }
const proposal = z.object({ base_revision: z.string().min(1).max(80), entries }).strict()
const verification = z.object({ digest: z.string().length(64), passed: z.literal(true),
  suite: z.string().min(1).max(120), evidence: z.string().min(1).max(1000) }).strict()
const approval = z.object({ digest: z.string().length(64) }).strict()
const limit = (maxSize: number) => bodyLimit({ maxSize, onError: (c) => c.json({ error: "payload too large" }, 413) })
// Hono reports stream overflow while json() reads. Preserve that signal instead of treating it as malformed JSON.
const json = (c: Context) => c.req.json().catch((err: unknown) => {
  if (err instanceof Error && err.name === "BodyLimitError") throw err
  return null
})

export function mount(app: Hono, deps: Deps, jwt: string, now: () => number) {
  const throttle = createThrottle({ windowMs: 60_000, max: 30 }, now)
  const at = () => new Date(now()).toISOString()
  async function attempt(c: Context, fn: () => Promise<Response>) {
    try { return await fn() } catch (err) {
      if (err instanceof Error && err.name === "BodyLimitError") return c.json({ error: "payload too large" }, 413)
      if (err instanceof AuthError) return c.json({ error: err.message }, err.status as 401 | 403)
      if (err instanceof Conflict) return c.json({ error: err.message }, 409)
      return c.json({ error: "web memory unavailable" }, 503)
    }
  }
  const admin = (fn: (c: Context, actor: string) => Promise<Response>) => (c: Context) => attempt(c, async () => {
    const claims = requireAdmin(authenticate(c.req.header("authorization"), jwt, now()))
    return fn(c, claims.sub)
  })
  app.get("/web-memory/v1/catalog", (c) => attempt(c, async () => {
    const value = catalog.parse((await deps.repo.read()).catalog)
    c.header("Cache-Control", "public, max-age=300")
    return c.json(value)
  }))
  app.post("/web-memory/v1/reports", limit(2048), (c) => attempt(c, async () => {
    if (!throttle.hit(clientIp(c)).allowed) return c.json({ error: "rate limited" }, 429)
    const value = report.safeParse(await json(c))
    if (!value.success) return c.json({ error: "invalid workflow report" }, 400)
    await deps.submit(value.data)
    return c.json({ accepted: true, status: "pending" }, 202)
  }))
  app.get("/admin/web-memory", admin(async (c) => c.json(await deps.repo.read())))
  app.get("/admin/web-memory/reports", admin(async (c) => c.json({ reports: (await deps.reports()).flatMap((value) => {
    const parsed = report.safeParse(value)
    return parsed.success ? [parsed.data] : []
  }) })))
  app.post("/admin/web-memory/candidates", limit(262144), admin(async (c, actor) => {
    const value = proposal.safeParse(await json(c))
    if (!value.success) return c.json({ error: "invalid catalog candidate" }, 400)
    const state = await deps.repo.update((state) => propose(state, value.data.base_revision, value.data.entries, actor, at()))
    return c.json(state.candidates.at(-1)!, 201)
  }))
  app.post("/admin/web-memory/candidates/:id/verify", limit(2048), admin(async (c, actor) => {
    const value = verification.safeParse(await json(c))
    if (!value.success) return c.json({ error: "invalid smoke verification" }, 400)
    const state = await deps.repo.update((state) => {
      const item = state.candidates.find((item) => item.id === c.req.param("id"))
      if (!item || item.published || item.digest !== value.data.digest || digest(item.entries) !== item.digest) throw new Conflict("candidate digest mismatch")
      item.verification = { digest: item.digest, suite: value.data.suite, evidence: value.data.evidence, actor, at: at() }
      return state
    })
    return c.json(state.candidates.find((item) => item.id === c.req.param("id"))!)
  }))
  app.post("/admin/web-memory/candidates/:id/publish", limit(1024), admin(async (c, actor) => {
    const value = approval.safeParse(await json(c))
    if (!value.success) return c.json({ error: "invalid approval" }, 400)
    const state = await deps.repo.update((state) => {
      const item = state.candidates.find((item) => item.id === c.req.param("id"))
      if (!item || item.published || item.digest !== value.data.digest || digest(item.entries) !== item.digest ||
        item.verification?.digest !== item.digest) throw new Conflict("verified candidate required")
      if (item.base_revision !== state.catalog.revision) throw new Conflict("stale base revision")
      const next = release(state, item.entries, actor, at(), { candidate: item.id })
      item.published = next.catalog.revision
      return next
    })
    return c.json(state.catalog)
  }))
  app.post("/admin/web-memory/rollback/:revision", limit(1024), admin(async (c, actor) => {
    const state = await deps.repo.update((state) => {
      const old = state.releases.find((item) => item.catalog.revision === c.req.param("revision"))
      if (!old) throw new Conflict("release not retained")
      return release(state, old.catalog.entries, actor, at(), { rollback: old.catalog.revision })
    })
    return c.json(state.catalog)
  }))
}
