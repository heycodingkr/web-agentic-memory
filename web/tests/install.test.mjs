import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { browserBridgeInstallCommand, defaultMcpEndpoint, normalizeMcpEndpoint, normalizeSourceRepository, remoteInstallCommand, setupCommands, sourceInstallCommands } from '../src/lib/install.js';

test('remote install copy uses the shared host templates and explicit Claude user scope', () => {
    assert.equal(remoteInstallCommand('codex', 'https://wam.example/mcp'), 'codex mcp add web-memory --url https://wam.example/mcp');
    assert.equal(remoteInstallCommand('claude-code', 'https://wam.example/mcp'), 'claude mcp add --transport http --scope user web-memory https://wam.example/mcp');
});
test('source downloads require an explicit valid repository URL', () => {
    assert.equal(normalizeSourceRepository(''), '');
    assert.equal(normalizeSourceRepository(' https://github.com/example/public-wam/ '), 'https://github.com/example/public-wam');
    for (const value of ['http://github.com/example/wam', 'https://user:secret@github.com/example/wam',
        'https://github.com/example/wam?token=secret', 'https://github.com/example/wam#readme',
        'https://github.com/example/wam/issues', 'https://github.com:444/example/wam',
        'https://github.com.evil.example/example/wam', 'javascript:alert(1)']) {
        assert.equal(normalizeSourceRepository(value), '', value);
    }
});

test('source uninstall names the installed Cargo package', () => {
    const packageNames = ['wam-cli', 'wam-mcp'].map(crate => {
        const manifest = readFileSync(new URL(`../../crates/${crate}/Cargo.toml`, import.meta.url), 'utf8');
        return manifest.match(/^name = "([^"]+)"/m)[1];
    });
    const page = readFileSync(new URL('../src/lib/components/SetupGuide.svelte', import.meta.url), 'utf8');
    const commands = [...page.matchAll(/cargo uninstall ([a-z-]+)/g)].map((match) => match[1]);
    assert.deepEqual(commands, packageNames);
    assert.ok(!sourceInstallCommands.includes('uninstall'));
    assert.ok(sourceInstallCommands.includes('cargo install --path crates/wam-mcp'));
});

test('every installation path uses the selected service and retains its path prefix', () => {
    for (const host of ['codex', 'claude-code']) {
        const commands = setupCommands(host, 'https://wam.example/team/mcp');
        assert.ok(commands.remote.includes('https://wam.example/team/mcp'));
        for (const key of ['local', 'link', 'doctor', 'recall', 'unlink']) {
            assert.ok(commands[key].includes('--endpoint https://wam.example/team'), key);
            assert.ok(!commands[key].includes('/team/mcp'), key);
            assert.ok(!commands[key].includes('127.0.0.1'), key);
        }
        assert.ok(commands.http.includes('https://wam.example/team/v1/memories/recall'));
        assert.ok(commands.remove.includes('remove'));
        assert.ok(!commands.remote.includes('remove'));
    }
});

test('invalid or credential-bearing endpoints cannot produce setup commands', () => {
    for (const endpoint of ['', 'javascript:alert(1)', 'https://user:secret@wam.example/mcp',
        'http://wam.example/mcp', 'https://wam.example/mcp?token=secret', 'https://wam.example/mcp#secret', 'https://wam.example/not-mcp']) {
        assert.equal(normalizeMcpEndpoint(endpoint), '', endpoint);
        assert.equal(setupCommands('codex', endpoint), null, endpoint);
    }
    assert.equal(normalizeMcpEndpoint(' http://127.0.0.1:3000/mcp '), 'http://127.0.0.1:3000/mcp');
    assert.equal(normalizeMcpEndpoint('http://[::1]:3000/mcp'), 'http://[::1]:3000/mcp');
});

test('default MCP address follows explicit MCP config, API config, then page origin', () => {
    assert.equal(defaultMcpEndpoint('https://web.example'), 'https://web.example/mcp');
    assert.equal(defaultMcpEndpoint('https://web.example', 'https://api.example/wam/'), 'https://api.example/wam/mcp');
    assert.equal(defaultMcpEndpoint('https://web.example', '/api'), 'https://web.example/api/mcp');
    assert.equal(defaultMcpEndpoint('https://web.example', 'https://api.example', 'https://mcp.example/mcp'), 'https://mcp.example/mcp');
});

test('PowerShell HTTP example avoids the curl alias and escapes endpoint punctuation', () => {
    const commands = setupCommands('claude-code', "https://wam.example/a'b/mcp", 'Windows');
    assert.ok(commands.http.startsWith('Invoke-RestMethod '));
    assert.ok(commands.http.includes("'https://wam.example/a''b/v1/memories/recall'"));
    assert.ok(commands.local.includes("'https://wam.example/a''b'"));
});

test('browser bridge registration uses explicit executables and never embeds credentials', () => {
    const command = browserBridgeInstallCommand('codex', 'https://wam.example/team/mcp', '/Users/test/.cargo/bin/wam', '/Applications/Arise.app/Contents/MacOS/agent_browser');
    assert.ok(command.startsWith('codex mcp add wam-browser -- /Users/test/.cargo/bin/wam'));
    assert.ok(command.includes('--endpoint https://wam.example/team browser-mcp'));
    assert.equal(browserBridgeInstallCommand('codex', 'https://wam.example/mcp', 'wam', '/browser'), '');
    assert.equal(browserBridgeInstallCommand('codex', 'https://user:secret@wam.example/mcp', '/wam', '/browser'), '');
    const windows = browserBridgeInstallCommand('claude-code', 'https://wam.example/mcp', 'C:\\Program Files\\Wam\\wam.exe', 'C:\\Arise\\agent_browser.exe', 'Windows');
    assert.ok(windows.startsWith('claude mcp add --scope user wam-browser --'));
    assert.ok(windows.includes("'C:\\Program Files\\Wam\\wam.exe'"));
    assert.ok(!windows.includes('WAM_TOKEN'));
});
test('installation copy safely quotes URL punctuation for POSIX shells and PowerShell', () => {
    const endpoint = "https://wam.example/a'b/mcp";
    assert.equal(remoteInstallCommand('codex', endpoint), "codex mcp add web-memory --url 'https://wam.example/a'\\''b/mcp'");
    assert.equal(remoteInstallCommand('codex', endpoint, 'Windows'), "codex mcp add web-memory --url 'https://wam.example/a''b/mcp'");
    assert.equal(remoteInstallCommand('codex', 'http://[::1]:3000/mcp'), "codex mcp add web-memory --url 'http://[::1]:3000/mcp'");
});
