import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { browserBridgeInstallCommand, claudeOnMuseEnv, defaultMcpEndpoint, firstRequest, museInstallCommand, museSettingsJson, museSettingsPath, normalizeMcpEndpoint, normalizeSourceRepository, remoteInstallCommand, setupCommands, sourceInstallCommands } from '../src/lib/install.js';

test('remote install copy uses the shared host templates and explicit Claude user scope', () => {
    assert.equal(remoteInstallCommand('codex', 'https://wam.example/mcp'), 'codex mcp add web-memory --url https://wam.example/mcp');
    assert.equal(remoteInstallCommand('claude-code', 'https://wam.example/mcp'), 'claude mcp add --transport http --scope user web-memory https://wam.example/mcp');
});
test('source repository URLs must be valid HTTPS GitHub roots', () => {
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

test('the public website defaults to the canonical API MCP endpoint without auth discovery', () => {
    for (const origin of ['https://webagenticmemory.com', 'https://www.webagenticmemory.com']) {
        const endpoint = defaultMcpEndpoint(origin);
        assert.equal(endpoint, 'https://api.webagenticmemory.com/mcp');
        assert.equal(setupCommands('codex', endpoint).remote, 'codex mcp add web-memory --url https://api.webagenticmemory.com/mcp');
        assert.ok(setupCommands('claude-code', endpoint).http.includes('https://api.webagenticmemory.com/v1/memories/recall'));
        assert.equal(defaultMcpEndpoint(origin, '/api'), origin + '/api/mcp');
        assert.equal(defaultMcpEndpoint(origin, '', 'https://other.example/mcp'), 'https://other.example/mcp');
    }
    assert.equal(defaultMcpEndpoint('http://127.0.0.1:5173'), 'http://127.0.0.1:5173/mcp');
    assert.equal(defaultMcpEndpoint('https://webagenticmemory.com.example'), 'https://webagenticmemory.com.example/mcp');
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
test('Muse Code is configured through its settings file, not a registration command', () => {
    const endpoint = 'https://wam.example/team/mcp';
    const commands = setupCommands('muse', endpoint);
    assert.deepEqual(JSON.parse(commands.remote), { schema_version: 1, mcpServers: { 'web-memory': { transport: 'streamable_http', url: endpoint } } });
    assert.equal(commands.remote, museSettingsJson(endpoint));
    for (const key of ['local', 'remove', 'instructions']) assert.equal(commands[key], '', key);
    assert.equal(commands.login, 'muse mcp login web-memory');
    assert.ok(commands.verify.startsWith('muse exec --disable-approval'));
    for (const key of ['link', 'doctor', 'recall', 'unlink']) assert.ok(commands[key].includes('--endpoint https://wam.example/team'), key);
    assert.equal(museSettingsPath, '~/.config/muse/settings.json');
});

test('Muse settings are only generated for valid endpoints and never carry credentials', () => {
    for (const endpoint of ['', 'https://user:secret@wam.example/mcp', 'https://wam.example/mcp?token=secret', 'http://wam.example/mcp', 'https://wam.example/not-mcp']) {
        assert.equal(museSettingsJson(endpoint), '', endpoint);
        assert.equal(setupCommands('muse', endpoint), null, endpoint);
    }
    assert.equal(JSON.parse(museSettingsJson('http://127.0.0.1:3000/mcp')).mcpServers['web-memory'].url, 'http://127.0.0.1:3000/mcp');
});

test('Muse Code installs with the official one-line installer for each platform', () => {
    assert.equal(museInstallCommand(), 'curl -fsSL https://dev.meta.ai/install.sh | sh');
    assert.equal(museInstallCommand('Linux'), 'curl -fsSL https://dev.meta.ai/install.sh | sh');
    assert.equal(museInstallCommand('Windows'), 'irm https://dev.meta.ai/install.ps1 | iex');
});

test('Claude Code on Muse Spark pins every model alias, keeps tool search off and reads the key from the environment', () => {
    const posix = claudeOnMuseEnv();
    assert.match(posix, /^export ANTHROPIC_BASE_URL="https:\/\/api\.meta\.ai"$/m);
    assert.match(posix, /^export ANTHROPIC_AUTH_TOKEN="\$MODEL_API_KEY"$/m);
    for (const name of ['ANTHROPIC_MODEL', 'ANTHROPIC_DEFAULT_OPUS_MODEL', 'ANTHROPIC_DEFAULT_SONNET_MODEL', 'ANTHROPIC_DEFAULT_HAIKU_MODEL', 'CLAUDE_CODE_SUBAGENT_MODEL']) {
        assert.match(posix, new RegExp(`^export ${name}="muse-spark-1\\.3"$`, 'm'), name);
    }
    assert.match(posix, /^export ENABLE_TOOL_SEARCH="false"$/m);
    const windows = claudeOnMuseEnv('Windows');
    assert.match(windows, /^\$env:ANTHROPIC_AUTH_TOKEN = \$env:MODEL_API_KEY$/m);
    assert.ok(!windows.includes('export '));
    for (const text of [posix, windows]) assert.ok(!/(sk-|Bearer |key=)/i.test(text), 'no secret-looking text is generated');
});

test('the first request fixes the recall arguments so models do not add filters of their own', () => {
    assert.ok(firstRequest.includes('exactly these arguments and no other fields'));
});

test('installation copy safely quotes URL punctuation for POSIX shells and PowerShell', () => {
    const endpoint = "https://wam.example/a'b/mcp";
    assert.equal(remoteInstallCommand('codex', endpoint), "codex mcp add web-memory --url 'https://wam.example/a'\\''b/mcp'");
    assert.equal(remoteInstallCommand('codex', endpoint, 'Windows'), "codex mcp add web-memory --url 'https://wam.example/a''b/mcp'");
    assert.equal(remoteInstallCommand('codex', 'http://[::1]:3000/mcp'), "codex mcp add web-memory --url 'http://[::1]:3000/mcp'");
});
