import type { MealDay, MealPlace } from './types';
import { isPreferredPlace, settings } from './settings.svelte';

export function placePrice(place: MealPlace): string | null {
  const price = place.meals.find((m) => m.price)?.price ?? null;
  if (!price) return null;
  const student = /학생\s*([\d,]+원)/.exec(price);
  return student ? student[1] : (/([\d,]+원)/.exec(price)?.[1] ?? null);
}

export function sortedPlaces(day: MealDay): MealPlace[] {
  const pref = settings.mealPlace;
  return [...day.places].sort((a, b) => Number(isPreferredPlace(b.name, pref)) - Number(isPreferredPlace(a.name, pref)));
}

function mealSlot(hour: number): '아침' | '점심' | '저녁' | null {
  return hour < 10 ? '아침' : hour < 15 ? '점심' : hour < 20 ? '저녁' : null;
}

export function isNowMeal(name: string, hour: number): boolean {
  const slot = mealSlot(hour);
  return slot !== null && name.startsWith(slot);
}

export function currentMealIndex(names: string[], hour: number): number {
  const slot = mealSlot(hour) ?? '저녁';
  const i = names.findIndex((n) => n.startsWith(slot));
  return i >= 0 ? i : hour >= 15 ? names.length - 1 : 0;
}
