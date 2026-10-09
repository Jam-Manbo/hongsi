import { api } from '../../shared/api/api';
import { Resource } from '../../shared/state/resource.svelte';
import type { MealDay } from '../../shared/types';

const MIN = 60_000;
export const meals = new Resource<MealDay[]>('meals', () => api.meals(), 30 * MIN);
