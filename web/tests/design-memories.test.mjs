import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import ts from 'typescript';
async function loadModule(source) {
 const { outputText } = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } });
 return import('data:text/javascript;base64,' + Buffer.from(outputText).toString('base64'));
}
const m=await loadModule(await readFile(new URL('../src/lib/memories.ts',import.meta.url),'utf8'));
const glyphSource=await readFile(new URL('../src/lib/components/SiteGlyph.svelte',import.meta.url),'utf8');
const {glyphFor}=await loadModule(glyphSource.match(/<script lang="ts" module>([\s\S]*?)<\/script>/)[1]);
test('design site pictograms recognize exact hosts and reject lookalikes',()=>{
 for(const host of ['figma.com','www.figma.com','canva.com','www.canva.com','stitch.withgoogle.com']) assert.equal(glyphFor(host,'compose-draft'),'design',host);
 for(const host of ['figma.com.evil.example','notfigma.com','stitch.withgoogle.com.evil.example']) assert.equal(glyphFor(host,'compose-draft'),'draft',host);
});
const seed=JSON.parse(await readFile(new URL('../../crates/wam-server/seed/design_memories.json',import.meta.url),'utf8'));
const catalog=seed.map(({id,payload:{l0,l1}})=>({
 memory_id:id,version_id:'v1',title:l1.title,summary:l1.summary,site:l0.site_host,domain:l0.site_host,
 task_category:l0.task_category,kind:l0.kind,status:'active',verified_date:'1793491200',
 canonical_url_patterns:l0.url_patterns.map(path=>'https://'+l0.site_host+path)
}));
test('real design templates appear in the Design category and URL searches stay within the product',()=>{
 assert.ok(catalog.every(row=>m.categoryOf(row)==='design'));
 for(const [url,key,id]of[
  ['https://www.figma.com/design/redacted-file','figma','web:example-figma-agent-responsive'],
  ['https://stitch.withgoogle.com/projects/redacted-project','stitch','web:example-stitch-mobile-app']
 ]){
  const hits=m.applyFilters(catalog,{query:url,category:'design',product:'all',task:'all',source:'examples'});
  assert.ok(hits.some(row=>row.memory_id===id),url);
  assert.ok(hits.every(row=>m.productOf(row).key===key));
 }
 assert.equal(m.applyFilters(catalog,{query:'https://figma.com.evil.example/design/test',category:'design',product:'all',task:'all',source:'examples'}).length,0);
});
test('published examples contain reusable instructions without private artifact identifiers or speed claims',()=>{
 assert.equal(new Set(seed.map(row=>row.id)).size,seed.length);
 const text=JSON.stringify(seed);
 assert.doesNotMatch(text,/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i);
 assert.doesNotMatch(text,/cef:[A-Z0-9-]{12,}|rpctoken=|access_token=|\/Users\//i);
 assert.doesNotMatch(text,/\d+(?:\.\d+)?\s*%\s*(?:faster|speed)/i);
 assert.ok(seed.some(row=>row.payload.l0.intent_key==='mobile-app-design'));
 assert.ok(seed.some(row=>row.payload.l0.intent_key==='export-design-frame'));
 for(const row of seed){
  assert.ok(row.payload.l1.title.startsWith('Example:'));
  assert.ok(row.payload.l2.recovery.length>0);
 }
});
