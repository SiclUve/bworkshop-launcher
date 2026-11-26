import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

const page = (name: string) => fileURLToPath(new URL(`./ui/${name}.html`, import.meta.url));

// Une page HTML par type de fenêtre (fenêtre principale, barre d'outils, barre de widget).
export default defineConfig({
  root: 'ui',
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: 'chrome120',
    rollupOptions: { input: { main: page('main'), widgetbar: page('widgetbar'), toolbar: page('toolbar') } },
  },
});
