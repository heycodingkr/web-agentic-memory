<script lang="ts">
  import { onMount } from "svelte"
  import { api } from "$lib/api"
  import toast from "svelte-french-toast"

  type Entry = { id: string; kind: string; site: string; title: string; summary: string; detail: string;
    evidence: string; tags: string[]; status: "approved"; verified_at: string }
  type Catalog = { schema_version: number; revision: string; entries: Entry[] }
  type Candidate = { id: string; digest: string; base_revision: string; entries: Entry[]; created_at: string;
    verification?: { suite: string; evidence: string }; published?: string }
  type Report = { domain: string; terminal: string; actions: { tool: string; outcome: string }[] }
  type State = { catalog: Catalog; candidates: Candidate[]; releases: { catalog: Catalog; at: string }[] }
  let state: State | null = null
  let reports: Report[] = []
  let draft: Entry[] = []
  let selected = ""
  let base = ""
  let tags = ""
  let suite = ""
  let evidence = ""
  let busy = false
  let error = ""
  $: entry = draft.find((item) => item.id === selected)

  onMount(() => { void load() })
  async function load(reset = true) {
    const values = await Promise.all([
      api<State>("/admin/web-memory"), api<{ reports: Report[] }>("/admin/web-memory/reports"),
    ]).catch((err: Error) => { error = err.message; return null })
    if (!values) return
    state = values[0]
    reports = values[1].reports
    error = ""
    if (reset) {
      base = state.catalog.revision
      draft = structuredClone(state.catalog.entries)
      select(draft[0]?.id ?? "")
    }
  }
  function select(id: string) {
    selected = id
    tags = draft.find((item) => item.id === id)?.tags.join(", ") ?? ""
  }
  function add(site = "") {
    const item: Entry = { id: crypto.randomUUID(), kind: "workflow", site, title: "", summary: "", detail: "",
      evidence: "", tags: [], status: "approved", verified_at: new Date().toISOString().slice(0, 10) }
    draft = [...draft, item]
    select(item.id)
  }
  function remove(id: string) {
    draft = draft.filter((item) => item.id !== id)
    select(draft[0]?.id ?? "")
  }
  function updateTags() {
    if (entry) entry.tags = tags.split(",").map((tag) => tag.trim()).filter(Boolean)
  }
  async function action(path: string, body: unknown, message: string, reset = false) {
    busy = true
    try {
      await api(path, { method: "POST", body: JSON.stringify(body) })
      toast.success(message)
      await load(reset)
    } catch (err) { toast.error(err instanceof Error ? err.message : "요청에 실패했습니다") }
    finally { busy = false }
  }
  function propose() {
    if (!state) return
    updateTags()
    void action("/admin/web-memory/candidates", { base_revision: base, entries: draft }, "검토 후보를 등록했습니다")
  }
</script>

<svelte:head><title>웹 에이전틱 메모리 — 관리자</title></svelte:head>
<div class="mb-6 flex flex-wrap items-center justify-between gap-3">
  <div>
    <h1 class="text-2xl font-bold">웹 에이전틱 메모리</h1>
    <p class="mt-2 text-sm text-gray-600">사용자 자료를 검토해 작업 방법을 작성하고, 테스트 결과를 기록한 뒤 승인합니다.</p>
  </div>
  <button class="rounded border bg-white px-4 py-2 text-sm" disabled={busy} on:click={() => load(false)}>새로고침</button>
</div>
{#if error}
  <p role="alert" class="mb-6 rounded border border-red-200 bg-red-50 p-4 text-sm text-red-800">메모리를 불러오지 못했습니다: {error}</p>
{/if}
{#if state}
  <p class="mb-5 text-xs text-gray-500">배포 버전: <span class="font-mono">{state.catalog.revision}</span> · {state.catalog.entries.length}개 작업 방법</p>
  <section class="mb-8 rounded-lg border bg-white p-5">
    <h2 class="mb-3 text-lg font-semibold">사용자 제공 자료</h2>
    <p class="mb-4 text-sm text-gray-600">도메인과 도구 실행 결과만 담은 검토 자료입니다. 보고서가 작업 방법으로 자동 등록되지는 않습니다.</p>
    {#if reports.length === 0}
      <p class="text-sm text-gray-500">아직 제공된 자료가 없습니다.</p>
    {:else}
      <div class="max-h-72 overflow-auto">
        <table class="w-full text-left text-sm">
          <thead><tr class="border-b text-gray-500"><th class="py-2">사이트</th><th>실행 결과</th><th>도구 순서</th><th></th></tr></thead>
          <tbody>{#each reports as report}
            <tr class="border-b align-top"><td class="py-3 pr-3">{report.domain}</td><td class="pr-3">{report.terminal}</td>
              <td class="pr-3">{report.actions.map((step) => step.tool + (step.outcome === "error" ? " (오류)" : "")).join(" → ")}</td>
              <td><button class="whitespace-nowrap text-indigo-700 underline" on:click={() => add(report.domain)}>작성 시작</button></td></tr>
          {/each}</tbody>
        </table>
      </div>
    {/if}
  </section>

  <section class="mb-8 rounded-lg border bg-white p-5">
    <div class="mb-4 flex items-center justify-between gap-2">
      <h2 class="text-lg font-semibold">작업 방법 편집</h2>
      <button class="rounded border px-3 py-2 text-sm" on:click={() => add()}>새 작업 방법</button>
    </div>
    <p class="mb-4 text-sm text-gray-600">여기서 편집한 내용은 후보로 등록됩니다. 승인할 때까지 현재 배포 내용은 유지됩니다.</p>
    <div class="grid gap-5 xl:grid-cols-[260px_1fr]">
      <div class="space-y-2">{#each draft as item (item.id)}
        <div class="flex gap-2 rounded border p-2 {selected === item.id ? 'border-indigo-500 bg-indigo-50' : ''}">
          <button class="min-w-0 flex-1 text-left text-sm" on:click={() => select(item.id)}>
            <span class="block truncate font-medium">{item.title || "새 작업 방법"}</span>
            <span class="block truncate text-xs text-gray-500">{item.site || "사이트 미입력"}</span>
          </button>
          <button class="text-xs text-red-700" aria-label={(item.title || "새 작업 방법") + " 제외"} on:click={() => remove(item.id)}>제외</button>
        </div>
      {/each}</div>
      {#if entry}
        <div class="grid gap-4 md:grid-cols-2" on:input={() => { draft = [...draft] }}>
          <label class="text-sm">사이트 도메인<input class="mt-1 w-full rounded border p-2" bind:value={entry.site} placeholder="medium.com" /></label>
          <label class="text-sm">종류<select class="mt-1 w-full rounded border p-2" bind:value={entry.kind}><option value="workflow">작업 절차</option><option value="site_note">사이트 참고</option><option value="gotcha">주의할 동작</option></select></label>
          <label class="text-sm md:col-span-2">제목<input class="mt-1 w-full rounded border p-2" maxlength="120" bind:value={entry.title} /></label>
          <label class="text-sm md:col-span-2">짧은 요령<textarea class="mt-1 w-full rounded border p-2" rows="3" maxlength="400" bind:value={entry.summary}></textarea></label>
          <label class="text-sm md:col-span-2">실행 절차<textarea class="mt-1 w-full rounded border p-2" rows="7" maxlength="8000" bind:value={entry.detail}></textarea></label>
          <label class="text-sm md:col-span-2">확인한 동작과 한계<textarea class="mt-1 w-full rounded border p-2" rows="3" maxlength="2000" bind:value={entry.evidence}></textarea></label>
          <label class="text-sm">태그 (쉼표 구분)<input class="mt-1 w-full rounded border p-2" bind:value={tags} on:change={updateTags} /></label>
          <label class="text-sm">확인 날짜<input class="mt-1 w-full rounded border p-2" type="date" bind:value={entry.verified_at} /></label>
        </div>
      {:else}<p class="text-sm text-gray-500">작업 방법을 추가하거나 왼쪽 목록에서 선택하세요. 비어 있는 후보를 승인하면 모든 작업 방법이 배포에서 제외됩니다.</p>{/if}
    </div>
    {#if base !== state.catalog.revision}<p class="mt-4 text-sm text-amber-800">최신 배포 내용이 바뀌었습니다. <button class="underline" on:click={() => load(true)}>최신 승인본으로 다시 시작</button></p>{/if}
    <button class="mt-5 rounded bg-gray-900 px-4 py-2 text-sm text-white disabled:opacity-50" disabled={busy || base !== state.catalog.revision} on:click={propose}>검토 후보 등록</button>
  </section>

  <section class="mb-8 rounded-lg border bg-white p-5">
    <h2 class="mb-3 text-lg font-semibold">후보 검증과 승인</h2>
    <p class="mb-4 text-sm text-gray-600">격리된 환경에서 테스트한 결과를 기록하세요. 기록은 해당 후보의 내용에만 적용됩니다.</p>
    <div class="mb-5 grid gap-3 md:grid-cols-2">
      <label class="text-sm">실행한 테스트<input class="mt-1 w-full rounded border p-2" bind:value={suite} placeholder="예: Medium 초안 저장·재열기" /></label>
      <label class="text-sm">결과 자료 위치<input class="mt-1 w-full rounded border p-2" bind:value={evidence} placeholder="예: 격리 스모크 결과 파일이나 링크" /></label>
    </div>
    <div class="space-y-4">{#each [...state.candidates].reverse() as item (item.id)}
      <div class="rounded border p-4">
        <div class="mb-2 flex flex-wrap items-center justify-between gap-3">
          <p class="text-sm">{new Date(item.created_at).toLocaleString()} · {item.entries.length}개 · {item.published ? "승인 완료" : item.verification ? "검증 기록됨" : "검증 대기"}</p>
          <div class="flex gap-3">
            <button class="text-sm text-indigo-700 disabled:text-gray-400" disabled={busy || !!item.published || !suite.trim() || !evidence.trim()}
              on:click={() => action("/admin/web-memory/candidates/" + item.id + "/verify", { digest: item.digest, passed: true, suite, evidence }, "검증 결과를 기록했습니다")}>검증 결과 기록</button>
            <button class="rounded bg-indigo-700 px-3 py-2 text-sm text-white disabled:opacity-40"
              disabled={busy || !!item.published || !item.verification || item.base_revision !== state.catalog.revision}
              on:click={() => action("/admin/web-memory/candidates/" + item.id + "/publish", { digest: item.digest }, "승인본을 배포했습니다", true)}>승인·배포</button>
          </div>
        </div>
        {#if item.base_revision !== state.catalog.revision && !item.published}<p class="mb-2 text-xs text-amber-800">배포 내용이 바뀌었습니다. 최신 내용으로 후보를 다시 등록하세요.</p>{/if}
        <details class="text-sm"><summary class="cursor-pointer">후보 내용 확인</summary>
          {#each item.entries as note}<article class="mt-3 border-t pt-3">
            <h3 class="font-semibold">{note.title} · {note.site}</h3><p class="mt-1">{note.summary}</p>
            <p class="mt-2 whitespace-pre-wrap text-gray-600">{note.detail}</p><p class="mt-2 text-xs text-gray-500">{note.evidence}</p>
          </article>{/each}
        </details>
      </div>
    {/each}</div>
  </section>
  <section class="rounded-lg border bg-white p-5">
    <h2 class="mb-3 text-lg font-semibold">최근 배포 기록</h2>
    {#each [...state.releases].reverse() as item}
      <div class="flex flex-wrap items-center justify-between gap-3 border-t py-3 text-sm">
        <span>{new Date(item.at).toLocaleString()} · {item.catalog.entries.length}개 <span class="ml-2 font-mono text-xs text-gray-500">{item.catalog.revision}</span></span>
        <button class="text-indigo-700 disabled:text-gray-400" disabled={busy || item.catalog.revision === state.catalog.revision}
          on:click={() => action("/admin/web-memory/rollback/" + item.catalog.revision, {}, "선택한 내용으로 새 버전을 배포했습니다", true)}>이 내용으로 되돌리기</button>
      </div>
    {/each}
  </section>
{/if}
