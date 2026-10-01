<script lang="ts">
  let { message, detail = '', compact = false }: { message: string; detail?: string; compact?: boolean } = $props();
</script>

<div class="empty-state" class:compact><span>{message}</span>{#if detail}<small>{detail}</small>{/if}</div>

<style>
  .empty-state {
    display: grid;
    place-items: center;
    min-height: 82px;
    padding: 20px 16px;
    border: 1.5px dashed var(--border-strong);
    border-radius: var(--radius);
    background: transparent;
    color: var(--text-3);
    font-size: 14px;
    line-height: 1.5;
    text-align: center;
    overflow-wrap: anywhere;
  }

  small { font-size: 13px; margin-top: 4px; }

  .compact { min-height: 64px; padding: 16px 12px; font-size: 13px; }
</style>
