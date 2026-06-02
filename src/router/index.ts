// ============================================================
// Router — Configuração de rotas da aplicação
// Cada GamePhase mapeia para uma rota/página específica
// ============================================================
import { createRouter, createWebHistory } from 'vue-router'
import type { RouteRecordRaw } from 'vue-router'

// Lazy-load: cada página carrega apenas quando necessário
const Dashboard   = () => import('../pages/Dashboard.vue')
const ChampSelect = () => import('../pages/ChampSelect.vue')
const PostGame    = () => import('../pages/PostGame.vue')
const Profile     = () => import('../pages/Profile.vue')
const FlashCard   = () => import('../pages/FlashCard.vue')
const Overlay     = () => import('../pages/Overlay.vue')
const Settings    = () => import('../pages/Settings.vue')
const RunePanel   = () => import('../pages/RunePanel.vue')
const BuildPanel  = () => import('../pages/BuildPanel.vue')

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    name: 'dashboard',
    component: Dashboard,
    meta: { title: 'Dashboard', phase: 'IDLE' },
  },
  {
    path: '/champ-select',
    name: 'champ-select',
    component: ChampSelect,
    meta: { title: 'Seleção de Campeões', phase: 'BAN_PHASE' },
  },
  {
    path: '/post-game',
    name: 'post-game',
    component: PostGame,
    meta: { title: 'Pós-Game', phase: 'POSTGAME' },
  },
  {
    path: '/profile',
    name: 'profile',
    component: Profile,
    meta: { title: 'Perfil' },
  },
  {
    path: '/flashcard',
    name: 'flashcard',
    component: FlashCard,
    meta: { title: 'Coach' },
  },
  {
    // Janela de overlay full-screen durante partidas:
    // FlashcardOverlay + WardOverlay + LoadingOverlay + ChampSelectPickOverlay
    path: '/overlay',
    name: 'overlay',
    component: Overlay,
    meta: { title: 'Overlay' },
  },
  {
    path: '/settings',
    name: 'settings',
    component: Settings,
    meta: { title: 'Configurações' },
  },
  {
    path: '/rune-panel',
    name: 'rune-panel',
    component: RunePanel,
    meta: { title: 'Runas' },
  },
  {
    path: '/build-panel',
    name: 'build-panel',
    component: BuildPanel,
    meta: { title: 'Build' },
  },
  // Fallback: redireciona para dashboard em rotas inválidas
  { path: '/:pathMatch(.*)*', redirect: '/' },
]

export const router = createRouter({
  history: createWebHistory(),
  routes,
})

// Atualiza o título da janela conforme a rota
router.afterEach((to) => {
  document.title = `RiftSync AI — ${to.meta.title ?? 'Dashboard'}`
})
