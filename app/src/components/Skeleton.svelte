<script lang="ts">
  let { rows = 3, height = 56 }: { rows?: number; height?: number } = $props();
</script>

<div class="skeleton" aria-busy="true" aria-label="불러오는 중">
  {#each Array(rows) as _, i (i)}
    <div class="bar" style:height="{height}px" style:animation-delay="{i * 90}ms"></div>
  {/each}
</div>

<style>
  .skeleton {
    display: grid;
    gap: 10px;
  }

  .bar {
    border-radius: 14px;
    background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
    background-size: 200% 100%;
    animation: shimmer 1.3s ease-in-out infinite;
  }

  @keyframes shimmer {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -200% 0;
    }
  }
</style>
