import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { watch: { ignored: ['**/target/**', '**/src-tauri/**', '**/core/**', '**/docs/**'] } },
});
