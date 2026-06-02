// ============================================================
// RiftSync AI — Entry point da aplicação Vue 3
// Monta o app, registra plugins (Pinia, Router) e estilos
// ============================================================
import { createApp } from 'vue'
import { createPinia } from 'pinia'

import App from './App.vue'
import { router } from './router'
import './style.css'

const app = createApp(App)

// Pinia: gerenciamento de estado global reativo
app.use(createPinia())

// Vue Router: navegação entre páginas / phases
app.use(router)

app.mount('#app')
