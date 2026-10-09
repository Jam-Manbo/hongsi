<script lang="ts">
  import type { Room } from '../../shared/types';

  let {
    room,
    mine = null,
    recent = new Set<number>(),
    selected = null,
    onpick,
  }: {
    room: Room;
    mine?: number | null;
    recent?: Set<number>;
    selected?: number | null;
    onpick: (seat: number) => void;
  } = $props();

  const cols = $derived(room.grid[0]?.length ?? 0);
  let width = $state(0);
  let scrolled = $state(0);
  let mapWidth = $state(0);
  const overflow = $derived(mapWidth + 28 > width + 4);
  const STATE = { free: '빈 자리', used: '사용 중', blocked: '이용 불가' } as const;
</script>

<div class="wrap" bind:clientWidth={width} onscroll={(e) => (scrolled = e.currentTarget.scrollLeft)}>
  <div class="map" bind:clientWidth={mapWidth} style:grid-template-columns="repeat({cols}, var(--cell))" role="group" aria-label="{room.name} 좌석 배치도">
    {#each room.grid as row, r (r)}
      {#each row as cell, c (`${r}-${c}`)}
        {#if cell}
          <button
            class="seat {cell.state}"
            class:mine={cell.no === mine}
            class:recent={recent.has(cell.no) && cell.no !== mine}
            class:sel={cell.no === selected}
            disabled={cell.state === 'blocked'}
            onclick={() => onpick(cell.no)}
            aria-label="{cell.no}번 {cell.no === mine ? '내 자리' : STATE[cell.state]}{recent.has(cell.no) ? ', 방금 배정됨' : ''}"
          >
            {cell.no}
          </button>
        {:else}
          <span class="gap" aria-hidden="true"></span>
        {/if}
      {/each}
    {/each}
  </div>
</div>

{#if overflow}
  <p class="hint">{scrolled > 20 ? '← 다시 밀어서 왼쪽 보기' : '좌우로 밀어서 전체 좌석 보기 →'}</p>
{/if}

<style>
  .wrap {
    overflow-x: auto;
    overflow-y: hidden;
    padding: 14px;
    border-radius: 16px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    -webkit-overflow-scrolling: touch;
  }

  .map {
    --cell: 36px;
    display: grid;
    gap: 4px;
    width: max-content;
    margin: 0 auto;
  }

  .seat,
  .gap {
    width: var(--cell);
    height: 34px;
  }

  .seat {
    border-radius: 7px;
    font-size: 11.5px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    transition:
      transform 0.12s,
      box-shadow 0.12s;
  }

  .seat.free {
    background: var(--seat-free);
    color: var(--on-seat-free);
  }

  .seat.used {
    background: var(--seat-used);
    color: var(--text-3);
  }

  .seat.blocked {
    background: var(--seat-blocked);
    color: transparent;
    cursor: default;
  }

  .seat:not(:disabled):hover {
    transform: scale(1.12);
    box-shadow: var(--shadow);
    z-index: 1;
  }

  .seat.recent {
    background: var(--warn-weak);
    color: var(--warn);
    box-shadow: inset 0 0 0 2px var(--warn);
    animation: pulse 1.6s ease-in-out infinite;
  }

  .seat.mine {
    background: var(--seat-mine);
    color: var(--on-primary);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--seat-mine) 35%, transparent);
  }

  .seat.sel {
    outline: 3px solid var(--text);
    outline-offset: 1px;
  }

  @keyframes pulse {
    50% {
      box-shadow: inset 0 0 0 2px var(--warn), 0 0 0 4px color-mix(in srgb, var(--warn) 25%, transparent);
    }
  }

  .hint {
    margin: 8px 4px 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
    text-align: right;
  }
</style>
