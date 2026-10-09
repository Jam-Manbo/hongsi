import { api } from '../../shared/api/api';
import { Resource } from '../../shared/state/resource.svelte';
import type { Todo } from '../../shared/types';

export const todos = new Resource<Todo[]>('todos', () => api.todos(), 5 * 60_000);
