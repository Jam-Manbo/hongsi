import { api } from '../../shared/api/api';
import { Resource } from '../../shared/state/resource.svelte';
import type { ClassNotification } from '../../shared/types';

export const notices = new Resource<ClassNotification[]>('notices', () => api.notifications(), 5 * 60_000);
