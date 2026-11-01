<script lang="ts">
  import type { Contribution } from '#lib/api';
  import { contributionStatus } from '#lib/my-page';
  import MyPageIcon from './MyPageIcon.svelte';
  let { item }: { item: Contribution } = $props();
  const status = $derived(contributionStatus(item));
</script>
<span class="status-badge {status.tone}"><MyPageIcon name={status.key === 'published' || status.key === 'supported' ? 'check' : status.action || status.key === 'rejected' ? 'alert' : 'clock'} size={13}/>{status.label}</span>
<style>
.status-badge{display:inline-flex;align-items:center;gap:5px;width:max-content;padding:5px 8px;border:1px solid #dfe5df;border-radius:6px;font-size:10px;font-weight:650;line-height:1.2;color:#52645a;background:#f1f4ef;white-space:nowrap}.green{color:#28603c;background:#edf6e9;border-color:#d8e9d0}.blue{color:#305d7e;background:#edf3f8;border-color:#dce6ef}.amber{color:#775619;background:#fbf3df;border-color:#eee0bc}.coral{color:#914b38;background:#fbede6;border-color:#efd8cc}
</style>
