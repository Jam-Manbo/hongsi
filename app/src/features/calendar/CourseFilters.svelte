<script lang="ts">
  import Icon from '../../shared/ui/Icon.svelte';
  import type { AcademicTerm, Course } from '../../shared/types';

  let { terms, showTerms, activeTerm, termCourses, termCourseIds, colors, hidden = $bindable(), selectedTerm = $bindable(), picker = false }: {
    terms: { key: string; label: string; term: AcademicTerm | null }[];
    showTerms: boolean;
    activeTerm: string;
    termCourses: Course[];
    termCourseIds: Set<number>;
    colors: Map<number, string>;
    hidden: Set<number>;
    selectedTerm: string | null;
    picker?: boolean;
  } = $props();
  const COMMON = -1;
  const termCoursesSelected = $derived([...termCourseIds].every((id) => !hidden.has(id)));

  function toggleCourse(id: number) {
    const next = new Set(hidden);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    hidden = next;
  }

  function selectTerm(key: string, button: HTMLButtonElement) {
    selectedTerm = key;
    const row = button.parentElement;
    if (!row) return;
    const bounds = row.getBoundingClientRect(), tab = button.getBoundingClientRect();
    if (tab.left < bounds.left) row.scrollLeft -= bounds.left - tab.left;
    else if (tab.right > bounds.right) row.scrollLeft += tab.right - bounds.right;
  }

  function termKeydown(event: KeyboardEvent) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    const button = event.currentTarget as HTMLButtonElement;
    const tabs = [...(button.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? [])];
    if (!tabs.length) return;
    event.preventDefault();
    const index = tabs.indexOf(button);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1
      : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    tabs[next].focus({ preventScroll: true });
    tabs[next].click();
  }

  function toggleAllCourses() {
    const next = new Set(hidden);
    for (const id of termCourseIds) {
      if (termCoursesSelected) next.add(id);
      else next.delete(id);
    }
    hidden = next;
  }

</script>

  <div class="course-filters" class:picker>
    {#if showTerms}
      <div class="term-tabs" role="tablist" aria-label="학기 필터">
        {#each terms as term (term.key)}
          <button class="term-tab" role="tab" id="course-term-{term.key}" aria-selected={activeTerm === term.key}
            aria-controls="course-filter-panel" tabindex={activeTerm === term.key ? 0 : -1}
            onclick={(event) => selectTerm(term.key, event.currentTarget)} onkeydown={termKeydown}>{term.label}</button>
        {/each}
      </div>
    {/if}
    <div id="course-filter-panel" class="legend" role={showTerms ? 'tabpanel' : 'group'}
      aria-labelledby={showTerms ? `course-term-${activeTerm}` : undefined} aria-label={showTerms ? undefined : '과목 필터'}>

      <button class="course all-courses" onclick={toggleAllCourses}>
        <Icon name={termCoursesSelected ? 'close' : 'tick'} size={14} />
        {termCoursesSelected ? '전체 해제' : '전체 선택'}
      </button>
      <button
        class="course"
        class:off={hidden.has(COMMON)}
        style:--c="var(--todo-neutral)"
        onclick={() => toggleCourse(COMMON)}
        aria-pressed={!hidden.has(COMMON)}
      >
        <span class="dot sq"></span>공통
      </button>
      {#each termCourses as c (c.id)}
        <button
          class="course"
          class:off={hidden.has(c.id)}
          style:--c={colors.get(c.id)}
          onclick={() => toggleCourse(c.id)}
          aria-pressed={!hidden.has(c.id)}
        >
          <span class="dot"></span>{c.name}
        </button>
      {/each}
    </div>
  </div>

<style>
  .course-filters { min-width: 0; }
  .term-tabs { display: flex; gap: 4px; max-width: 100%; overflow-x: auto; overscroll-behavior-x: contain; scrollbar-width: thin; margin-bottom: 12px; border-bottom: 1px solid var(--border); }
  .term-tab { flex: none; min-height: 44px; padding: 10px 12px; border-bottom: 2px solid transparent; color: var(--text-3); font-size: 13px; font-weight: 650; white-space: nowrap; }
  .term-tab[aria-selected='true'] { color: var(--primary-text); border-bottom-color: var(--primary); }
  .term-tab:focus-visible { outline: 2px solid var(--primary); outline-offset: -3px; border-radius: 6px; }
  .course-filters.picker .legend { display: grid; gap: 8px; margin: 0; padding: 0; overflow: visible; }
  .course-filters.picker .course { width: 100%; border-radius: 12px; min-height: 44px; padding: 10px 12px; text-align: left; line-height: 1.5; overflow-wrap: anywhere; min-width: 0; }
  .course-filters.picker .course .dot { flex: none; }
  .course-filters.picker .course.all-courses { justify-content: center; }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 12px;
  }
  .course {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 0 11px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 650;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    transition: opacity 0.15s;
  }
  .course .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--c);
  }
  .course .dot.sq {
    border-radius: 2px;
  }
  .course.off {
    opacity: 0.4;
    text-decoration: line-through;
  }
  .course.all-courses {
    color: var(--primary-text);
    background: var(--primary-weak);
    border-color: color-mix(in srgb, var(--primary) 25%, var(--border));
  }
  @media (max-width: 639px) {
  .legend {
      flex-wrap: nowrap;
      overflow-x: auto;
      scrollbar-width: none;
      margin: 0 -16px 12px;
      padding: 0 16px;
    }
  .legend::-webkit-scrollbar {
      display: none;
    }
  .course {
      flex: none;
    }
  }
</style>
