import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';

// The READMEs are the first thing a visitor reads. Keep both languages in one structure, keep the MCP tool table equal to
// the real tool catalog, and keep every local link and anchor working.
const rootUrl = new URL('../../', import.meta.url);
const read = (relative) => readFileSync(new URL(relative, rootUrl), 'utf8');

const readmes = [
    { file: 'README.md', toolsHeading: 'MCP tools' },
    { file: 'README.ko.md', toolsHeading: 'MCP 도구' }
].map((entry) => ({ ...entry, text: read(entry.file) }));

const withoutCode = (text) => text.replace(/^```[\s\S]*?^```/gm, '');
const headingsOf = (text) =>
    [...withoutCode(text).matchAll(/^(#{1,6})\s+(.+?)\s*$/gm)].map((match) => ({ level: match[1].length, title: match[2] }));

// GitHub's anchor rule: lower-case, drop punctuation, spaces become hyphens.
const slug = (title) => title.trim().toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-');

function catalogTools() {
    const source = read('crates/wam-mcp/src/lib.rs');
    const start = source.indexOf('pub fn tool_catalog()');
    assert.notEqual(start, -1, 'tool_catalog() not found in crates/wam-mcp/src/lib.rs');
    const next = source.indexOf('\npub fn ', start + 1);
    const body = source.slice(start, next === -1 ? undefined : next);
    return [...body.matchAll(/"name":\s*"(web_memory_[a-z_]+)"/g)].map((match) => match[1]).sort();
}

test('the README tool tables list exactly the MCP tool catalog', () => {
    const expected = catalogTools();
    assert.equal(expected.length, 10, 'the READMEs say ten tools');
    for (const { file, text, toolsHeading } of readmes) {
        const lines = text.split('\n');
        const start = lines.findIndex((line) => line === `### ${toolsHeading}`);
        assert.notEqual(start, -1, `${file} has no "### ${toolsHeading}" section`);
        const rows = [];
        for (let index = start + 1; index < lines.length && !lines[index].startsWith('#'); index += 1) {
            if (lines[index].startsWith('|')) rows.push(lines[index]);
        }
        const listed = rows.flatMap((row) => [...row.matchAll(/`(web_memory_[a-z_]+)`/g)].map((match) => match[1])).sort();
        assert.deepEqual(listed, expected, `${file} tool table is out of date`);
    }
});

test('English and Korean READMEs share one heading structure', () => {
    const [english, korean] = readmes.map(({ text }) => headingsOf(text).map((heading) => heading.level));
    assert.deepEqual(korean, english);
});

test('README local links and anchors resolve', () => {
    for (const { file, text } of readmes) {
        const anchors = new Set(headingsOf(text).map((heading) => slug(heading.title)));
        for (const match of withoutCode(text).matchAll(/\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)) {
            const target = match[1];
            if (/^(https?:|mailto:)/.test(target)) continue;
            if (target.startsWith('#')) {
                assert.ok(anchors.has(target.slice(1)), `${file}: no heading for ${target}`);
            } else {
                assert.ok(existsSync(new URL(target.split('#')[0], rootUrl)), `${file}: missing file ${target}`);
            }
        }
    }
});
