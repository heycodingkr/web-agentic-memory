<script lang="ts">let { name, size = 20 }: { name: string; size?: number } = $props();</script>
<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">
{#if name === 'points'}<ellipse cx="12" cy="5" rx="7" ry="3"/><path d="M5 5v5c0 1.7 3.1 3 7 3s7-1.3 7-3V5M5 10v5c0 1.7 3.1 3 7 3s7-1.3 7-3v-5M5 15v4c0 1.7 3.1 3 7 3s7-1.3 7-3v-4"/>
{:else if name === 'rank'}<path d="M8 3h8v7a4 4 0 0 1-8 0V3ZM8 5H4v3a4 4 0 0 0 4 4M16 5h4v3a4 4 0 0 1-4 4M12 14v6M8 21h8"/>
{:else if name === 'memory'}<path d="M6 3h8l4 4v14H6zM14 3v5h4M9 12h6M9 16h6"/>
{:else if name === 'clock' || name === 'history'}<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>
{:else if name === 'alert'}<path d="m12 3 10 18H2zM12 9v5M12 17h.01"/>
{:else if name === 'check'}<circle cx="12" cy="12" r="9"/><path d="m8 12 3 3 5-6"/>
{:else if name === 'refresh'}<path d="M20 8a8 8 0 0 0-14-3L3 8m0-5v5h5M4 16a8 8 0 0 0 14 3l3-3m0 5v-5h-5"/>
{:else if name === 'search'}<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/>
{:else if name === 'close'}<path d="m6 6 12 12M6 18 18 6"/>
{:else if name === 'chart'}<path d="M4 21V11M10 21V5M16 21v-9M22 21V2"/>
{:else if name === 'plus'}<path d="M12 5v14M5 12h14"/>
{:else if name === 'user'}<circle cx="12" cy="8" r="4"/><path d="M4 22v-3a8 8 0 0 1 16 0v3"/>
{:else if name === 'chevron'}<path d="m9 5 7 7-7 7"/>
{:else}<path d="M5 12h14m-5-5 5 5-5 5"/>{/if}
</svg>
