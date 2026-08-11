import { mount } from 'svelte';
import App from './App.svelte';
import './styles.css';

document.addEventListener('contextmenu', (event) => {
  event.preventDefault();
});

mount(App, {
  target: document.getElementById('app')!,
});
