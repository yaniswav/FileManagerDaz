import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  
  // Prevent vite from obscuring rust errors
  clearScreen: false,
  
  // Tauri expects a fixed port
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**']
    }
  },
  
  // Env variables starting with TAURI_ are exposed to tauri
  envPrefix: ['VITE_', 'TAURI_']
});
