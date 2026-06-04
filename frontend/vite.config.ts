import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

const frontendPort = Number(process.env.FRONTEND_PORT ?? '43178');
const backendPort = Number(process.env.BACKEND_PORT ?? '43177');
const backendTarget = process.env.PUBLIC_API_BASE
  ? new URL(process.env.PUBLIC_API_BASE).origin
  : `http://localhost:${backendPort}`;

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    port: frontendPort,
    strictPort: true,
    proxy: {
      '/api': {
        target: backendTarget,
        changeOrigin: true
      }
    }
  }
});
