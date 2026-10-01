<script lang="ts">
  import { setItemAlert } from '../lib/actions.svelte';
  import { itemStatus, isFinished } from '../lib/colors';
  import { dateTime, dday, dueDateTime } from '../lib/format';
  import { cleanHtml as clean } from '../lib/html';
  import { leadSummary } from '../lib/settings.svelte';
  import type { CalendarItem, Todo } from '../lib/types';
  import FileList from './FileList.svelte';
  import SubmitSheet from './SubmitSheet.svelte';
  import Switch from './Switch.svelte';
  import TodoRow from './TodoRow.svelte';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import EmptyState from './EmptyState.svelte';

  let {
    item,
    course,
    color,
    ontoggle,
    onclose,
    subtodos = [],
    onaddtodo,
    oneditodo,
  }: {
    item: CalendarItem | null;
    course: string;
    color: string;
    ontoggle: (item: CalendarItem) => void;
    onclose: () => void;
    subtodos?: Todo[];
    onaddtodo?: (item: CalendarItem) => void;
    oneditodo?: (todo: Todo) => void;
  } = $props();

  let open = $state(false);
  let submitOpen = $state(false);
  const canSubmit = $derived(item?.kind === 'assignment' && !!item.submit?.files);
  const submitted = $derived(item?.status === 'submitted');
  const key = $derived(item?.key ?? null);

  $effect(() => {
    open = key !== null;
  });

  const cmid = $derived(item ? Number(item.key.split(':')[1]) : 0);

  const status = $derived(item ? itemStatus(item) : null);
  const alertable = $derived(!!item && item.due !== null && item.due * 1000 > Date.now() && !item.done);
</script>

<Sheet bind:open title={item?.kind === 'vod' ? '온라인 강의' : '과제'} {onclose}>
  {#if item && status}
    <div class="head" style:--c={color}>
      <span class="course"><span class="dot"></span>{course}</span>
      <h3>{item.title}</h3>
      <div class="chips">
        <span class="chip {status.tone}">{status.label}</span>
        {#if item.due && !isFinished(item)}<span class="chip primary">{dday(item.due).label}</span>{/if}
      </div>
    </div>

    <dl class="facts">
      {#if item.kind === 'vod'}
        {#if item.start}<div><dt>인정 시작</dt><dd>{dateTime(item.start)}</dd></div>{/if}
        {#if item.due}<div><dt>출석 인정 마감</dt><dd class="strong">{dueDateTime(item.due)}</dd></div>{/if}
        {#if item.lateUntil}<div><dt>기간 외 시청</dt><dd>{dueDateTime(item.lateUntil)}까지</dd></div>{/if}
        {#if item.watch}
          <div><dt>요구 시간</dt><dd>{item.watch.required ?? '-'}</dd></div>
          <div><dt>내 시청 시간</dt><dd>{item.watch.watched ?? '기록 없음'}</dd></div>
          {#if item.watch.mark}<div><dt>출석부 표시</dt><dd>{item.watch.mark}</dd></div>{/if}
        {/if}
      {:else}
        {#if item.start}<div><dt>제출 시작</dt><dd>{dateTime(item.start)}</dd></div>{/if}
        {#if item.due}
          <div>
            <dt>마감</dt>
            <dd class="strong">
              {dueDateTime(item.due)}
            </dd>
          </div>
        {/if}
        {#if item.lateUntil && item.lateUntil !== item.due}<div><dt>늦은 제출 마감</dt><dd>{dueDateTime(item.lateUntil)}</dd></div>{/if}
      {/if}
      {#if alertable}
        <div class="alert-row">
          <dt>마감 알림</dt>
          <dd>
            {#if item.alert !== false}<span class="lead">{leadSummary() || '알림 시간 없음'}</span>{/if}
            <Switch checked={item.alert !== false} label="이 {item.kind === 'vod' ? '강의' : '과제'} 마감 알림" onchange={(on) => item && setItemAlert(item, on)} />
          </dd>
        </div>
      {/if}
    </dl>

    {#if canSubmit}
      <a class="cn2-link" href={item.url} target="_blank" rel="noopener noreferrer">클래스룸에서 열기 <Icon name="external" size={14} /></a>
    {/if}

    {#if onaddtodo}
      <div class="subtodos">
        <div class="sub-head">
          <strong>이 {item.kind === 'vod' ? '강의' : '과제'}의 할 일</strong>
          {#if subtodos.length}<span class="muted">{subtodos.filter((t) => t.doneAt !== null).length}/{subtodos.length}</span>{/if}
          <button class="sub-add" onclick={() => item && onaddtodo?.(item)}><Icon name="plus" size={15} stroke={2.4} />추가</button>
        </div>
        {#each subtodos as t (t.id)}
          <TodoRow todo={t} {color} {course} onopen={(x) => oneditodo?.(x)} />
        {:else}
          <EmptyState message="추가한 할 일이 없어요" compact />
        {/each}
      </div>
    {/if}

    {#if item.kind === 'assignment'}
      <div class="intro">
        {#if item.introHtml?.trim()}
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html clean(item.introHtml)}
        {:else}
          <p class="muted">설명이 없는 과제예요.</p>
        {/if}
      </div>
      {#if item.attachments.length}
        <div class="files">
          <FileList files={item.attachments} source={(index) => ({ kind: 'assign', cmid, index })} {course} />
        </div>
      {/if}
    {/if}
  {/if}

  {#snippet footer()}
    {#if item}
      <button class="btn w1 {item.done ? 'btn-ghost' : 'btn-soft'}" onclick={() => item && ontoggle(item)} aria-label={item.done ? '완료 체크 해제' : '완료로 체크'}>
        <Icon name="tick" size={18} />{item.done ? '체크 해제' : '완료 체크'}
      </button>
      {#if canSubmit}
        <button class="btn btn-primary w1" onclick={() => (submitOpen = true)}>
          <Icon name="check" size={18} />{submitted ? '과제 수정하기' : '제출하기'}
        </button>
      {:else}
        <a class="btn btn-primary w1" href={item.url} target="_blank" rel="noopener noreferrer">
          클래스룸에서 열기 <Icon name="external" size={17} />
        </a>
      {/if}
    {/if}
  {/snippet}
</Sheet>

<SubmitSheet bind:open={submitOpen} {item} />

<style>
  .head {
    display: grid;
    gap: 8px;
    padding: 4px 0 14px;
  }

  .course {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--c);
  }

  h3 {
    font-size: 20px;
    font-weight: 750;
    letter-spacing: -0.02em;
    line-height: 1.35;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .facts {
    margin: 0 0 14px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--surface-2);
    display: grid;
    gap: 8px;
  }

  .facts div {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 14px;
  }

  dt {
    color: var(--text-3);
    flex: none;
  }

  dd {
    margin: 0;
    text-align: right;
    font-weight: 550;
    font-variant-numeric: tabular-nums;
  }

  dd.strong {
    font-weight: 750;
  }

  .cn2-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin: -4px 0 14px;
    font-size: 13px;
    font-weight: 650;
    color: var(--primary-text);
    text-decoration: none;
  }

  .subtodos {
    display: grid;
    gap: 6px;
    margin: 0 0 16px;
  }

  .sub-head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
  }

  .sub-add {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 30px;
    padding: 0 10px;
    border-radius: 999px;
    background: var(--primary-weak);
    color: var(--primary-text);
    font-size: 12.5px;
    font-weight: 700;
  }

  .intro {
    font-size: 14.5px;
    line-height: 1.7;
    color: var(--text);
    overflow-wrap: anywhere;
  }

  .intro :global(img) {
    max-width: 100%;
    height: auto;
    border-radius: 8px;
  }

  .intro :global(table) {
    display: block;
    overflow-x: auto;
    border-collapse: collapse;
  }

  .intro :global(td),
  .intro :global(th) {
    border: 1px solid var(--border);
    padding: 4px 8px;
  }

  .intro :global(p) {
    margin: 0 0 8px;
  }

  .files {
    margin-top: 14px;
  }

  .facts .alert-row {
    align-items: center;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  .alert-row dd {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .lead {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-3);
  }
</style>
