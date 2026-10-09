import { api } from '../../shared/api/api';
import { Resource } from '../../shared/state/resource.svelte';
import type { SeatSession, SeatsData } from '../../shared/types';

const MIN = 60_000;
export const seats = new Resource<SeatsData>('seats', () => api.seats(), MIN);
export const seatSession = new Resource<{ session: SeatSession | null }>('seat-session', () => api.seatSession(), MIN);
