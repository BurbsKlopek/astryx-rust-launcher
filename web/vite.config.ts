import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  base: './',
  plugins: [react()],
  build: {
    target: 'es2020',
    cssCodeSplit: false,
    assetsInlineLimit: 4096,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules/react') || id.includes('node_modules/scheduler')) {
            return 'react-vendor';
          }
          if (id.includes('node_modules/@a2ui') || id.includes('node_modules/zod')) {
            return 'a2ui-vendor';
          }
          if (id.includes('node_modules/@astryxdesign') || id.includes('node_modules/@stylexjs')) {
            return 'astryx-vendor';
          }
        },
      },
    },
  },
});
