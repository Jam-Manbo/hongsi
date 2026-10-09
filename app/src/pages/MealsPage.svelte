<script lang="ts">
  import { onMount } from 'svelte';
  import { hourNow, todayKey } from '../shared/utils/format';
  import { isNowMeal, placePrice, sortedPlaces } from '../features/meals/meals';
  import { meals } from '../features/meals/meal-resource.svelte';
  import EmptyState from '../shared/ui/EmptyState.svelte';
  import Icon from '../shared/ui/Icon.svelte';
  import LoadError from '../shared/ui/LoadError.svelte';
  import Skeleton from '../shared/ui/Skeleton.svelte';

  let picked = $state('');

  onMount(() => {
    meals.load();
  });

  const days = $derived(meals.data ?? []);
  const day = $derived(days.find((d) => d.date === (picked || todayKey())) ?? days[0]);

  const isNow = (name: string) => day?.date === todayKey() && isNowMeal(name, hourNow());
</script>

<div class="page">
  <LoadError resource={meals} what="학식을" />
  {#if !meals.data}
    {#if !meals.error}
      <Skeleton rows={1} height={60} />
      <div style="height: 12px"></div>
      <Skeleton rows={3} height={140} />
    {/if}
  {:else if day}
    <div class="days" role="tablist" aria-label="요일">
      {#each days as d (d.date)}
        <button role="tab" class="day" aria-selected={d.date === day.date} onclick={() => (picked = d.date)}>
          <span class="wd">{d.weekday.slice(0, 1)}</span>
          <span class="dt">{Number(d.date.slice(8))}</span>
          {#if d.date === todayKey()}<span class="dot" aria-label="오늘"></span>{/if}
        </button>
      {/each}
    </div>

    {#each sortedPlaces(day) as place (place.name)}
      <h2 class="section-title place">
        <span>{place.name}</span>
        {#if placePrice(place)}<span class="chip primary">{placePrice(place)}</span>{/if}
      </h2>
      <div class="meals">
        {#each place.meals as meal (meal.name)}
          <article class="meal card" class:now={isNow(meal.name)}>
            <header>
              <div class="title">
                <h3>{meal.name}</h3>
                {#if isNow(meal.name)}<span class="chip primary">지금</span>{/if}
              </div>
              {#if meal.start}<span class="time"><Icon name="clock" size={14} />{meal.start}~{meal.end}</span>{/if}
            </header>
            <ul style:--menu-rows={Math.max(7, Math.ceil(meal.items.length / 2))}>
              {#each meal.items as food, i (i)}
                <li>{food}</li>
              {/each}
            </ul>
          </article>
        {/each}
      </div>
    {:else}
      <EmptyState message="이날은 메뉴가 없어요." detail="주말·공휴일에는 운영하지 않을 수 있어요." />
    {/each}
  {:else}
    <EmptyState message="이날은 메뉴가 없어요." detail="주말·공휴일에는 운영하지 않을 수 있어요." />
  {/if}
</div>

<style>
  .days {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 4px;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }

  .day {
    position: relative;
    display: grid;
    justify-items: center;
    gap: 2px;
    padding: 10px 0;
    border-radius: 14px;
    background: transparent;
    border: 1px solid transparent;
    transition: background 0.15s, color 0.15s;
  }

  .wd {
    font-size: 12px;
    font-weight: 650;
    color: var(--text-3);
  }

  .dt {
    font-size: 18px;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }

  .dot {
    position: absolute;
    top: 7px;
    right: 9px;
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: var(--seat-mine);
  }

  .day[aria-selected='true'] {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--on-primary);
  }

  .day[aria-selected='true'] .wd {
    color: inherit;
    opacity: 0.85;
  }

  .meals {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 10px;
  }

  .meal {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px;
  }

  .meal.now {
    border-color: var(--primary);
    box-shadow: none;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  h3 {
    font-size: 16px;
    font-weight: 750;
  }

  .time {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12.5px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    align-content: start;
    gap: 6px;
    flex: 1;
  }

  li {
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: 14px;
    font-weight: 500;
    line-height: 1.45;
    letter-spacing: -0.01em;
    color: var(--text);
  }

  @media (max-width: 767px) {
    .meals {
      grid-template-columns: minmax(0, 1fr);
    }

    ul {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      grid-template-rows: repeat(var(--menu-rows), minmax(1.55em, auto));
      grid-auto-flow: column;
      align-items: start;
      gap: 5px 12px;
      font-size: 13px;
    }

    li {
      font-size: inherit;
      line-height: 1.55;
      letter-spacing: -0.02em;
    }
  }


  .place {
    justify-content: flex-start;
    gap: 8px;
    font-size: 15px;
    color: var(--text);
  }

  .meal header { padding-bottom: 12px; border-bottom: 1px solid var(--border); }
  .day[aria-selected='true'] .dot { background: var(--on-primary); }

</style>
