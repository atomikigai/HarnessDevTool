import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

const frontendPort = Number(process.env.FRONTEND_PORT ?? '43178');
const backendPort = Number(process.env.BACKEND_PORT ?? '43177');
const backendTarget = process.env.PUBLIC_API_BASE
  ? new URL(process.env.PUBLIC_API_BASE).origin
  : `http://localhost:${backendPort}`;
const apiToken = process.env.PUBLIC_HARNESS_API_TOKEN ?? process.env.HARNESS_API_TOKEN ?? '';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    port: frontendPort,
    strictPort: true,
    proxy: {
      '/api': {
        target: backendTarget,
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq, req) => {
            if (
              apiToken.trim().length > 0 &&
              ['POST', 'PUT', 'PATCH', 'DELETE'].includes(req.method ?? '')
            ) {
              proxyReq.setHeader('Authorization', `Bearer ${apiToken.trim()}`);
            }
          });
        }
      }
    }
  }
});
