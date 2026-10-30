import templates from '../../../guides/install-templates.json' with { type: 'json' };

/** @param {string} value @param {string} platform */
function quote(value, platform) {
    if (/^[a-zA-Z0-9_./:=-]+$/.test(value)) return value;
    return platform === 'Windows'
        ? "'" + value.replaceAll("'", "''") + "'"
        : "'" + value.replaceAll("'", "'\\''") + "'";
}

/** @param {'codex' | 'claude-code'} host @param {string} endpoint @param {string} platform */
export function remoteInstallCommand(host, endpoint, platform = 'macOS') {
    const template = templates.hosts[host];
    const args = template.remote.map((arg) =>
        arg.replaceAll('{mcp_endpoint}', endpoint).replaceAll('{scope}', 'user'));
    return [template.program, ...args].map((arg) => quote(arg, platform)).join(' ');
}

/** @param {string} value */
export function normalizeMcpEndpoint(value) {
    try {
        const url = new URL(value.trim());
        const loopback = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
        if ((url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback)) ||
            url.username || url.password || url.search || url.hash || !url.pathname.endsWith('/mcp')) return '';
        return url.origin + url.pathname;
    } catch {
        return '';
    }
}

/** @param {string} origin @param {string} apiBase @param {string} configured */
export function defaultMcpEndpoint(origin, apiBase = '', configured = '') {
    if (configured) return configured;
    const base = apiBase.replace(/\/$/, '');
    return new URL(base ? base + '/mcp' : '/mcp', origin).href;
}

export const sourceInstallCommands = [
    'cargo install --path crates/wam-cli',
    'cargo install --path crates/wam-mcp',
    'wam --version'
].join('\n');

/** All examples keep the service's optional path prefix and never contain credentials.
 * @param {'codex' | 'claude-code'} host @param {string} mcpEndpoint @param {string} platform
 */
export function setupCommands(host, mcpEndpoint, platform = 'macOS') {
    const endpoint = normalizeMcpEndpoint(mcpEndpoint);
    if (!endpoint) return null;
    const apiBase = endpoint.slice(0, -'/mcp'.length);
    const wam = `wam --endpoint ${quote(apiBase, platform)}`;
    const template = templates.hosts[host];
    const hostCommand = (/** @type {string[]} */ args) => [template.program, ...args]
        .map(arg => quote(arg.replaceAll('{scope}', 'user'), platform)).join(' ');
    const payload = JSON.stringify({ url: 'https://example.com/guide', task_category: 'compose-draft', keywords: ['blog', 'write'], limit: 3 });
    // PowerShell's curl alias and native argument quoting vary by version; use its own HTTP client.
    const http = platform === 'Windows'
        ? `Invoke-RestMethod -Method Post -Uri ${quote(apiBase + '/v1/memories/recall', platform)} -ContentType 'application/json' -Body ${quote(payload, platform)}`
        : `curl --fail-with-body ${quote(apiBase + '/v1/memories/recall', platform)} -H 'Content-Type: application/json' --data ${quote(payload, platform)}`;
    return {
        remote: remoteInstallCommand(host, endpoint, platform),
        local: `${wam} install ${host} --local`,
        verify: hostCommand(template.get),
        login: host === 'codex' ? 'codex mcp login web-memory' : '/mcp',
        link: `${wam} link`,
        doctor: `${wam} doctor --json`,
        recall: `${wam} --anonymous recall https://example.com/guide --task compose-draft --keyword blog --keyword write --json`,
        instructions: `wam guide print --host ${host}`,
        remove: hostCommand(template.remove),
        unlink: `${wam} unlink`,
        http
    };
}

/** Optional Arise browser MCP bridge; paths come from the user's own local installation.
 * @param {'codex' | 'claude-code'} host @param {string} endpoint @param {string} wamBinary @param {string} browserBinary @param {string} platform
 */
export function browserBridgeInstallCommand(host, endpoint, wamBinary, browserBinary, platform = 'macOS') {
    const normalized = normalizeMcpEndpoint(endpoint);
    const absolute = (/** @type {string} */ value) => !/[\r\n\0]/.test(value) &&
        (platform === 'Windows' ? /^[A-Za-z]:[\\/]/.test(value) : value.startsWith('/'));
    if (!normalized || !absolute(wamBinary) || !absolute(browserBinary)) return '';
    const prefix = host === 'codex' ? ['codex', 'mcp', 'add', 'wam-browser'] : ['claude', 'mcp', 'add', '--scope', 'user', 'wam-browser'];
    return [...prefix, '--', wamBinary, '--endpoint', normalized.slice(0, -4), 'browser-mcp', '--browser-mcp', browserBinary, '--browser-arg=--mcp-server']
        .map(arg => quote(arg, platform)).join(' ');
}
