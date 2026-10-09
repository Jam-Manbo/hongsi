<script lang="ts">
  let {
    checked,
    label,
    disabled = false,
    busy = false,
    onchange,
  }: { checked: boolean; label: string; disabled?: boolean; busy?: boolean; onchange: (next: boolean) => void } = $props();
</script>

<button type="button" class="sw" role="switch" aria-checked={checked} aria-label={label} aria-disabled={disabled || busy} aria-busy={busy} {disabled} onclick={() => { if (!disabled && !busy) onchange(!checked); }}>
  <span class="knob" aria-hidden="true"></span>
</button>

<style>
  .sw {
    position: relative;
    flex: none;
    width: 46px;
    height: 28px;
    border-radius: 999px;
    background: var(--border-strong);
    transition: background 0.18s;
  }

  .sw[aria-checked='true'] {
    background: var(--primary);
  }

  .sw:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .sw[aria-busy='true']:not(:disabled) {
    cursor: wait;
  }

  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 22px;
    height: 22px;
    border-radius: 999px;
    background: #fff;
    box-shadow: 0 1px 3px rgb(0 0 0 / 25%);
    transition: transform 0.2s var(--ease);
  }

  .sw[aria-checked='true'] .knob {
    transform: translateX(18px);
  }
</style>
