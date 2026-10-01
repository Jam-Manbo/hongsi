<script lang="ts">
  import { api } from '../lib/api';
  import { dueDateTime } from '../lib/format';
  import { errorText, writeBlocked } from '../lib/net.svelte';
  import { calendar, handleAuthError } from '../lib/store.svelte';
  import { toast } from '../lib/ui.svelte';
  import type { CalendarItem, SubmissionView } from '../lib/types';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import Skeleton from './Skeleton.svelte';

  let { open = $bindable(false), item }: { open: boolean; item: CalendarItem | null } = $props();

  let view = $state<SubmissionView | null>(null);
  let error = $state('');
  let keep = $state(new Set<string>());
  let added = $state<File[]>([]);
  let statement = $state(false);
  let lateChecked = $state(false);
  let step = $state<'edit' | 'confirm'>('edit');
  let busy = $state(false);
  let picker: HTMLInputElement | undefined = $state();

  const cmid = $derived(item ? Number(item.key.split(':')[1]) : 0);

  $effect(() => {
    if (!open || !cmid) return;
    view = null;
    error = '';
    added = [];
    statement = false;
    lateChecked = false;
    step = 'edit';
    api
      .submission(cmid)
      .then((v) => {
        view = v;
        keep = new Set(v.info.files.map((f) => f.name));
      })
      .catch((e) => {
        if (!handleAuthError(e)) error = errorText(e, '제출 정보를 읽지 못했어요');
      });
  });

  const total = $derived(keep.size + added.length);
  const max = $derived(view?.config.maxFiles ?? 1);
  const SEND_LIMIT = 100 * 1024 * 1024;
  const perFile = $derived(Math.min(view && view.config.maxBytes > 0 ? view.config.maxBytes : Infinity, SEND_LIMIT));
  const addedBytes = $derived(added.reduce((n, f) => n + f.size, 0));
  const tooBig = $derived(added.filter((f) => f.size > perFile));
  const edited = $derived(view?.info.status === 'submitted');
  const canSend = $derived(
    !!view &&
      !view.closed &&
      total > 0 &&
      total <= max &&
      tooBig.length === 0 &&
      addedBytes <= SEND_LIMIT &&
      (!view.config.statement || statement),
  );

  const MB = 1024 * 1024;
  const size = (n: number | null) => {
    if (n === null) return '';
    if (n >= 1024 * MB) return `${+(n / 1024 / MB).toFixed(1)}GB`;
    if (n >= MB) return `${+(n / MB).toFixed(1)}MB`;
    return `${Math.max(1, Math.round(n / 1024))}KB`;
  };

  function pick(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    added = [...added, ...Array.from(input.files ?? [])];
    input.value = '';
  }

  function toggleKeep(name: string) {
    const next = new Set(keep);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    keep = next;
  }

  async function send() {
    if (!view || !item) return;
    if (writeBlocked('school', '제출할')) return;
    busy = true;
    try {
      const v = await api.submit(cmid, [...keep], added, view.late && lateChecked, statement);
      view = v;
      if (calendar.data) {
        calendar.set({
          ...calendar.data,
          items: calendar.data.items.map((i) => (i.key === item!.key ? { ...i, status: 'submitted', done: true } : i)),
        });
      }
      toast(edited ? '과제를 수정했어요' : '과제를 제출했어요', 'success', 4000);
      open = false;
    } catch (e) {
      if (!handleAuthError(e)) {
        error = errorText(e, '제출하지 못했어요');
        step = 'edit';
      }
    } finally {
      busy = false;
    }
  }
</script>

<Sheet bind:open title={step === 'confirm' ? (view?.late ? '마감이 지난 과제예요' : '제출할까요?') : edited ? '과제 수정하기' : '과제 제출'}>
  {#if error}<div class="error-box"><Icon name="alert" size={18} />{error}</div>{/if}
  {#if !view && !error}
    <Skeleton rows={3} height={52} />
  {:else if view && step === 'edit'}
    <div class="due" class:late={view.late}>
      <Icon name={view.late ? 'alert' : 'clock'} size={17} />
      {#if view.due}마감 {dueDateTime(view.due)}{:else}마감 없음{/if}
      {#if view.late}<span class="chip danger">마감 지남 · 지각 제출</span>{/if}
    </div>
    {#if view.closed}
      <p class="closed">
        {view.cutoff && view.cutoff * 1000 < Date.now() ? '제출 마감이 끝나서 더 이상 제출할 수 없어요.' : '지금은 이 과제를 제출하거나 고칠 수 없어요.'}
      </p>
    {:else}
      {#if view.info.files.length}
        <h4>{edited ? '지금 제출된 파일' : '첨부된 파일'} <span class="muted">빼려면 누르세요</span></h4>
        <div class="list">
          {#each view.info.files as f (f.name)}
            <button class="file" class:off={!keep.has(f.name)} onclick={() => toggleKeep(f.name)} aria-pressed={keep.has(f.name)}>
              <Icon name={keep.has(f.name) ? 'tick' : 'close'} size={16} stroke={2.4} />
              <span class="name">{f.name}</span>
              <span class="muted">{keep.has(f.name) ? size(f.size) : '뺄 예정'}</span>
            </button>
          {/each}
        </div>
      {/if}

      <h4>
        새로 올릴 파일
        <span class="muted">
          {total}/{max}개 · 파일당 {size(perFile)}까지{view.config.maxBytes > SEND_LIMIT ? ` (클래스룸 설정 ${size(view.config.maxBytes)})` : ''}
        </span>
      </h4>
      {#if total > max}<p class="warn-line">파일을 {max}개까지만 낼 수 있어요.</p>{/if}
      {#if addedBytes > SEND_LIMIT}<p class="warn-line">큰 파일은 클래스룸에서 직접 올려 주세요.</p>{/if}
      <div class="list">
        {#each added as f, i (f.name + i)}
          <div class="file new" class:bad={tooBig.includes(f)}>
            <Icon name="file" size={16} />
            <span class="name">{f.name}</span>
            <span class="muted">{tooBig.includes(f) ? '너무 커요' : size(f.size)}</span>
            <button class="x" onclick={() => (added = added.filter((_, j) => j !== i))} aria-label="{f.name} 빼기"><Icon name="close" size={15} /></button>
          </div>
        {/each}
        <button class="add" onclick={() => picker?.click()} disabled={total >= max}>
          <Icon name="plus" size={17} stroke={2.4} />{total >= max ? `최대 ${max}개예요` : '파일 고르기'}
        </button>
        <input bind:this={picker} type="file" multiple hidden onchange={pick} />
      </div>

      {#if view.config.statement}
        <label class="statement">
          <input type="checkbox" bind:checked={statement} />
          다른 사람의 것을 베끼지 않았음을 확인해요 (제출 서약)
        </label>
      {/if}
      {#if view.config.text && !view.config.files}
        <p class="warn">텍스트 입력으로 내는 과제예요. 클래스룸에서 제출해 주세요.</p>
      {/if}
    {/if}
  {:else if view && step === 'confirm'}
    {#if view.late}
      <div class="late-box">
        <Icon name="alert" size={22} />
        <div>
          <strong>마감({view.due ? dueDateTime(view.due) : ''})이 지났어요.</strong>
          <p>지금 {edited ? '제출을 고치면' : '제출하면'} 클래스룸에 <b>지각 제출</b>로 표시돼요. {edited ? '기한 안에 낸 기존 제출도 지각으로 바뀔 수 있어요.' : ''}</p>
        </div>
      </div>
      <label class="statement strong">
        <input type="checkbox" bind:checked={lateChecked} />
        지각 제출로 기록되는 것을 확인했어요
      </label>
    {:else}
      <p class="confirm">클래스룸에 바로 {edited ? '고친 파일로 다시 제출' : '제출'}돼요. 파일 {total}개를 보낼게요.</p>
    {/if}
  {/if}

  {#snippet footer()}
    {#if step === 'edit'}
      <button class="btn btn-ghost w1" onclick={() => (open = false)}>닫기</button>
      <button class="btn btn-primary w2" disabled={!canSend} onclick={() => (step = 'confirm')}>
        <Icon name="check" size={18} />{edited ? '과제 수정하기' : '제출하기'}
      </button>
    {:else}
      <button class="btn btn-ghost w1" onclick={() => (step = 'edit')} disabled={busy}>뒤로</button>
      <button class="btn {view?.late ? 'btn-danger' : 'btn-primary'} w2" disabled={busy || (view?.late && !lateChecked)} onclick={send}>
        {busy ? '보내는 중…' : view?.late ? '지각 제출하기' : '제출하기'}
      </button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .error-box { margin-bottom: 14px; }
  .due {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--surface-2);
    font-size: 14px;
    font-weight: 650;
    margin-bottom: 14px;
  }

  .due.late {
    background: var(--danger-weak);
    color: var(--danger);
  }

  h4 {
    display: flex;
    justify-content: space-between;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-2);
    margin: 14px 2px 8px;
  }

  h4 .muted {
    font-weight: 550;
  }

  .list {
    display: grid;
    gap: 6px;
  }

  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    font-size: 13.5px;
    text-align: left;
  }

  .file .name {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .file .muted {
    font-size: 12px;
  }

  .file.off {
    opacity: 0.55;
    text-decoration: line-through;
  }

  .file.new {
    border-style: dashed;
  }

  .file.bad {
    border-color: var(--danger);
    color: var(--danger);
  }

  .x {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    color: var(--text-3);
  }

  .add {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 46px;
    border-radius: 12px;
    border: 1.5px dashed var(--border-strong);
    color: var(--primary-text);
    font-weight: 700;
    font-size: 14px;
  }

  .add:disabled {
    color: var(--text-3);
  }

  .warn,
  .closed {
    margin-top: 10px;
    font-size: 13.5px;
    color: var(--warn);
    font-weight: 600;
  }

  .closed {
    color: var(--danger);
  }

  .statement {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    margin-top: 14px;
    font-size: 13.5px;
    color: var(--text-2);
  }

  .statement input {
    margin-top: 3px;
    width: 18px;
    height: 18px;
    accent-color: var(--primary);
  }

  .statement.strong {
    font-weight: 700;
    color: var(--danger);
  }

  .statement.strong input {
    accent-color: var(--danger);
  }

  .late-box {
    display: flex;
    gap: 12px;
    padding: 14px;
    border-radius: 14px;
    background: var(--danger-weak);
    color: var(--danger);
  }

  .late-box p {
    margin-top: 4px;
    font-size: 14px;
    line-height: 1.6;
    color: var(--text);
  }

  .confirm {
    font-size: 14.5px;
    color: var(--text-2);
    line-height: 1.6;
  }

  .warn-line {
    margin: -2px 0 8px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--danger);
  }
</style>
