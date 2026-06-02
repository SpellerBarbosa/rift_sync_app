import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

// Tauri exposes TAURI_DEV_HOST for mobile dev, or undefined on desktop
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [
    vue(),
    tailwindcss(),
  ],

  // Evita que o Vite obscureça erros do Rust durante o desenvolvimento
  clearScreen: false,

  server: {
    host: host || false,
    port: 1420,
    strictPort: true,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      // Ignora arquivos Rust para evitar reloads desnecessários
      ignored: ["**/src-tauri/**"],
    },
  },
});
