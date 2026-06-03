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
const Settings    = () => import('../pages/Settings.vue')
const RunePanel      = () => import('../pages/RunePanel.vue')
const BuildPanel     = () => import('../pages/BuildPanel.vue')
const InGameBuild    = () => import('../pages/InGameBuild.vue')
const WardWindow     = () => import('../pages/WardWindow.vue')
const MatchupWindow  = () => import('../pages/MatchupWindow.vue')
const ChampPickWindow= () => import('../pages/ChampPickWindow.vue')
const LoadingWindow  = () => import('../pages/LoadingWindow.vue')

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
  {
    path: '/ingame-build',
    name: 'ingame-build',
    component: InGameBuild,
    meta: { title: 'Build In-Game' },
  },
  { path: '/ward-overlay',      name: 'ward-overlay',      component: WardWindow,      meta: { title: 'Wards' } },
  { path: '/matchup-overlay',   name: 'matchup-overlay',   component: MatchupWindow,   meta: { title: 'Matchup' } },
  { path: '/champ-pick-overlay',name: 'champ-pick-overlay',component: ChampPickWindow, meta: { title: 'Pick' } },
  { path: '/loading-overlay',   name: 'loading-overlay',   component: LoadingWindow,   meta: { title: 'Loading' } },
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
