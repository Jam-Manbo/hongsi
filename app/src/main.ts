import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { applyTheme } from './lib/settings.svelte';
import { isApp } from './lib/env';

document.documentElement.dataset.platform = isApp ? 'app' : 'web';
applyTheme();

export default mount(App, { target: document.getElementById('app')! });
