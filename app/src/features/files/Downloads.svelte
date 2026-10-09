<script lang="ts">
  import { isApp } from '../../shared/api/api';
  import { downloads, openDownload, revealDownload } from './download-state.svelte';
  import { ago } from '../../shared/utils/format';
  import EmptyState from '../../shared/ui/EmptyState.svelte';
  import Icon from '../../shared/ui/Icon.svelte';
  import Sheet from '../../shared/ui/Sheet.svelte';
  let open = $state(false);
</script>

<button class="icon-btn" onclick={() => open = true} aria-label="받은 파일" title="받은 파일"><Icon name="inbox" size={23} /></button>
<Sheet bind:open title="받은 파일">
  {#if downloads.list.length}
    <ul class="files">
      {#each downloads.list as d (d.id + d.at)}
        <li>
          <span class="ficon"><Icon name="file" size={18} /></span>
          <div class="finfo">
            <strong>{d.name}</strong>
            <span class="muted">{d.course} · {ago(d.at / 1000)}</span>
          </div>
          <div class="fbtns" class:one={!isApp}>
            <button class="btn btn-soft sm" onclick={() => openDownload(d)}>바로 열기</button>
            {#if isApp}
              <button class="btn btn-ghost sm" onclick={() => revealDownload(d)}>폴더에서 열기</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <EmptyState message="아직 받은 파일이 없어요." detail="과제 상세에서 첨부 파일을 다운로드하면 여기에 모여요." />
  {/if}

</Sheet>
<style>
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }

  li {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 12px;
    align-items: center;
    padding: 12px;
    border-radius: 14px;
    border: 1px solid var(--border);
    background: var(--surface-2);
  }

  .ficon {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    background: var(--primary-weak);
    color: var(--primary-text);
  }

  .finfo {
    display: grid;
    min-width: 0;
  }

  .finfo strong {
    font-size: 14px;
    font-weight: 650;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .finfo span {
    font-size: 12px;
  }

  .fbtns {
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .fbtns.one {
    grid-template-columns: 1fr;
  }

  .sm {
    min-height: 40px;
    font-size: 13.5px;
    border-radius: 10px;
  }

</style>
