<script lang="ts">
  import { ALERT_LEADS, settings } from '../settings/settings.svelte';
  import Switch from '../../shared/ui/Switch.svelte';

  let { enabled, leads, label, disabled = false, ontoggle, onchange }: {
    enabled: boolean;
    leads: number[] | null;
    label: string;
    disabled?: boolean;
    ontoggle: (on: boolean) => void | Promise<void>;
    onchange: (leads: number[]) => void | Promise<void>;
  } = $props();

  let busy = $state(false);
  const selected = $derived(leads ?? settings.alertLeads);
  const locked = $derived(disabled || busy);

  async function change(fn: () => void | Promise<void>) {
    if (locked) return;
    busy = true;
    try { await fn(); } finally { busy = false; }
  }

  function toggle(min: number) {
    const next = selected.includes(min) ? selected.filter((m) => m !== min) : [...selected, min];
    return change(() => onchange(next.sort((a, b) => b - a)));
  }
</script>

<section class="deadline-alerts" aria-label={label} aria-busy={locked}>
  <div class="heading">
    <strong>마감 알림</strong>
    <Switch checked={enabled} {label} busy={locked} onchange={(on) => change(() => ontoggle(on))} />
  </div>
  <div class="leads" class:off={!enabled} role="group" aria-label="이 항목의 마감 알림 시간">
    {#each ALERT_LEADS as lead (lead.min)}
      <button type="button" class="filter" aria-pressed={selected.includes(lead.min)} disabled={!enabled} aria-disabled={!enabled || locked} onclick={() => toggle(lead.min)}>{lead.label}</button>
    {/each}
  </div>
  {#if enabled && selected.length === 0}<p class="hint">알림을 받을 시간을 선택해 주세요.</p>{/if}
</section>

<style>
  .deadline-alerts { display: grid; gap: 10px; padding: 14px; border: 1px solid var(--border); border-radius: 14px; background: var(--surface-2); }
  .heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .heading strong { font-size: 14px; }
  .leads { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 4px; }
  .leads .filter { min-width: 0; padding: 0 2px; font-size: 12px; white-space: nowrap; }
  .leads .filter[aria-disabled='true']:not(:disabled) { cursor: wait; }
  .off { opacity: 0.5; }
  .hint { font-size: 12px; line-height: 1.5; color: var(--text-3); }
</style>
