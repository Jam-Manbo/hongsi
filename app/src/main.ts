import './shared/state/resource-lifecycle';
import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { applyTheme } from './features/settings/settings.svelte';
import { isApp } from './platform/env';

document.documentElement.dataset.platform = isApp ? 'app' : 'web';
applyTheme();

export default mount(App, { target: document.getElementById('app')! });
