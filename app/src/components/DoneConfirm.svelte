<script lang="ts">
  import { cancelDoneConfirmation, confirmDone, doneConfirmation } from '../lib/actions.svelte';
  import { itemStatus } from '../lib/colors';
  import { calendar } from '../lib/store.svelte';
  import Sheet from './Sheet.svelte';

  const pending = $derived(doneConfirmation.pending);
  const item = $derived(calendar.data?.items.find((i) => i.key === pending?.key));
  const status = $derived(item ? itemStatus(item) : null);
  let open = $state(false);

  $effect(() => {
    open = !!pending && !!item;
    if (pending && !item) cancelDoneConfirmation();
  });
</script>

<Sheet bind:open title={pending?.done ? '완료로 표시할까요?' : '완료 체크를 해제할까요?'} layer={1} onclose={cancelDoneConfirmation}>
  {#if item && status}
    <div class="content">
      <strong class="item-title">{item.title}</strong>
      <div class="status"><span>클래스룸 상태</span><span class="chip {status.tone}">{status.label}</span></div>
      <p>홍시에서 이 {item.kind === 'vod' ? '강의를' : '과제를'} {pending?.done ? '완료' : '미완료'}로 표시해요. 클래스룸의 {item.kind === 'vod' ? '시청·출석' : '제출'} 상태는 바뀌지 않아요.</p>
    </div>
  {/if}
  {#snippet footer()}
    <button class="btn btn-ghost w1" onclick={cancelDoneConfirmation}>취소</button>
    <button class="btn btn-primary w2" onclick={confirmDone} disabled={!item}>{pending?.done ? '완료로 표시' : '체크 해제'}</button>
  {/snippet}
</Sheet>

<style>
  .content { display: grid; gap: 14px; }
  .item-title { font-size: 16px; line-height: 1.5; overflow-wrap: anywhere; }
  .status { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; font-size: 13px; color: var(--text-2); }
  p { font-size: 14px; line-height: 1.7; color: var(--text-2); }
</style>
