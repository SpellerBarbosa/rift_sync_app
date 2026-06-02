<script setup lang="ts">
// ============================================================
// InGameBuildOverlay — Build compacta visível durante a partida
//
// Posição: canto superior esquerdo (top: 10px, left: 10px).
// Ocupa ~220px × 90px — área livre acima do portrait do campeão.
//
// Conteúdo:
//   Linha 1 — ícone do campeão  +  keystone  +  itens do core build  +  botas
//   Linha 2 — ordem de habilidades  +  badge de tier/role
//
// Pode ser colapsado com um clique no header.
// ============================================================

import { ref, computed, watch, onMounted } from 'vue'
import { useGameBuildStore } from '../stores/gameBuild'
import { useGameStateStore } from '../stores/gameState'

const gameBuild = useGameBuildStore()
const gameState = useGameStateStore()

const collapsed = ref(false)

const visible = computed(() =>
  gameState.phase === 'INGAME' && !!gameBuild.build
)

// ── DDragon ──────────────────────────────────────────────────
const DD_BASE = 'https://ddragon.leagueoflegends.com/cdn'
const ddVer   = ref('16.10.1')

async function loadVersion() {
  try {
    const r = await fetch('https://ddragon.leagueoflegends.com/api/versions.json')
    const v: string[] = await r.json()
    ddVer.value = v[0]
  } catch { /* usa fallback */ }
}
onMounted(loadVersion)

function itemUrl(id: number)   { return `${DD_BASE}/${ddVer.value}/img/item/${id}.png` }
function champUrl(key: string) { return `${DD_BASE}/${ddVer.value}/img/champion/${key}.png` }

// ── Rune helpers ─────────────────────────────────────────────
interface RuneEntry { id: number; icon: string; name: string }
interface SlotEntry { runes: RuneEntry[] }
interface PathEntry { id: number; icon: string; slots: SlotEntry[] }

const runeTree = ref<PathEntry[]>([])

watch(ddVer, async () => {
  if (runeTree.value.length) return
  try {
    const r = await fetch(`${DD_BASE}/${ddVer.value}/data/pt_BR/runesReforged.json`)
    runeTree.value = await r.json()
  } catch { /* continua sem runas */ }
}, { immediate: true })

function runeIconById(perkId: number): string {
  for (const path of runeTree.value) {
    for (const slot of path.slots) {
      const r = slot.runes.find(r => r.id === perkId)
      if (r) return `${DD_BASE}/img/${r.icon}`
    }
  }
  return ''
}

// ── Computed: melhor keystone, core build, botas, skills ─────

const topKeystone = computed(() => {
  const ks = gameBuild.build?.runes?.keystones
  if (!ks?.length) return null
  return [...ks].sort((a, b) => b.frequency - a.frequency)[0]
})

const topCoreBuild = computed(() => {
  const cb = gameBuild.build?.items?.coreBuilds
  if (!cb?.length) return null
  return [...cb].sort((a, b) => b.frequency - a.frequency)[0]
})

const topBoots = computed(() => {
  const bt = gameBuild.build?.items?.boots
  if (!bt?.length) return null
  return [...bt].sort((a, b) => b.frequency - a.frequency)[0]
})

const topSkillOrder = computed(() => {
  const sp = gameBuild.build?.skills?.skillPriority
  if (!sp?.length) return null
  return [...sp].sort((a, b) => b.frequency - a.frequency)[0]
})

const skillParts = computed(() =>
  topSkillOrder.value?.order.split(' > ') ?? []
)
</script>

<template>
  <Transition name="igb-fade">
    <div v-if="visible" class="igb-root" :class="{ 'igb-collapsed': collapsed }">

      <!-- ── Tab de colapso ────────────────────────────────── -->
      <button class="igb-toggle" @click="collapsed = !collapsed" :title="collapsed ? 'Expandir build' : 'Recolher build'">
        <img
          v-if="gameBuild.champion?.key"
          :src="champUrl(gameBuild.champion.key)"
          class="igb-champ-icon"
          @error="($event.target as HTMLImageElement).style.opacity = '.3'"
        />
        <span class="igb-collapse-arrow">{{ collapsed ? '▶' : '◀' }}</span>
      </button>

      <!-- ── Painel expandido ──────────────────────────────── -->
      <Transition name="igb-expand">
        <div v-if="!collapsed" class="igb-panel">

          <!-- Linha 1: Keystone + itens core + botas -->
          <div class="igb-row">
            <!-- Keystone -->
            <div v-if="topKeystone" class="igb-keystone-wrap">
              <img
                :src="runeIconById(topKeystone.perkId)"
                class="igb-keystone"
                @error="($event.target as HTMLImageElement).style.opacity = '.2'"
              />
            </div>

            <div class="igb-row-divider" />

            <!-- Core items -->
            <template v-if="topCoreBuild">
              <div
                v-for="itemId in topCoreBuild.items.slice(0, 3)"
                :key="itemId"
                class="igb-item-wrap"
              >
                <img
                  :src="itemUrl(itemId)"
                  class="igb-item"
                  @error="($event.target as HTMLImageElement).style.opacity = '.2'"
                />
              </div>
            </template>

            <!-- Botas -->
            <div v-if="topBoots" class="igb-item-wrap igb-boots">
              <img
                :src="itemUrl(topBoots.itemId)"
                class="igb-item"
                @error="($event.target as HTMLImageElement).style.opacity = '.2'"
                title="Botas recomendadas"
              />
            </div>
          </div>

          <!-- Linha 2: Skill order + badge -->
          <div class="igb-row igb-row-bottom">
            <div class="igb-skills">
              <span
                v-for="(s, i) in skillParts"
                :key="i"
                class="igb-skill"
              >{{ s }}</span>
            </div>

            <div class="igb-badge-wrap">
              <span v-if="gameBuild.build?.role" class="igb-badge-role">
                {{ gameBuild.build.role }}
              </span>
              <span class="igb-badge-sc">SC</span>
            </div>
          </div>

        </div>
      </Transition>

    </div>
  </Transition>
</template>

<style scoped>
/* ── Root: canto superior esquerdo ───────────────────────── */
.igb-root {
  position:       fixed;
  top:            10px;
  left:           10px;
  z-index:        25;
  display:        flex;
  align-items:    flex-start;
  gap:            0;
  pointer-events: auto;
  user-select:    none;
}

/* ── Tab de toggle (sempre visível) ──────────────────────── */
.igb-toggle {
  display:        flex;
  flex-direction: column;
  align-items:    center;
  justify-content: center;
  gap:            4px;
  width:          32px;
  padding:        6px 4px;
  background:     rgba(5, 10, 24, 0.88);
  border:         1px solid rgba(200, 155, 60, 0.20);
  border-right:   none;
  border-radius:  3px 0 0 3px;
  cursor:         pointer;
  transition:     background .15s;
}
.igb-toggle:hover { background: rgba(10, 18, 36, 0.95); }

.igb-champ-icon {
  width:         24px;
  height:        24px;
  border-radius: 3px;
  object-fit:    cover;
  opacity:       0.85;
}

.igb-collapse-arrow {
  font-size:   7px;
  color:       rgba(200, 155, 60, 0.40);
  line-height: 1;
}

/* ── Painel principal ────────────────────────────────────── */
.igb-panel {
  display:        flex;
  flex-direction: column;
  gap:            5px;
  padding:        7px 10px;
  background:     rgba(5, 10, 24, 0.92);
  border:         1px solid rgba(200, 155, 60, 0.18);
  border-radius:  0 3px 3px 0;
  backdrop-filter: blur(10px);
  min-width:      180px;
}

/* ── Linhas ──────────────────────────────────────────────── */
.igb-row {
  display:     flex;
  align-items: center;
  gap:         4px;
}

.igb-row-bottom {
  justify-content: space-between;
}

.igb-row-divider {
  width:      1px;
  height:     22px;
  background: rgba(200, 155, 60, 0.15);
  flex-shrink: 0;
  margin:     0 2px;
}

/* ── Keystone ────────────────────────────────────────────── */
.igb-keystone-wrap {
  width:         30px;
  height:        30px;
  border-radius: 50%;
  background:    rgba(200, 155, 60, 0.08);
  border:        1px solid rgba(200, 155, 60, 0.25);
  display:       flex;
  align-items:   center;
  justify-content: center;
  flex-shrink:   0;
}
.igb-keystone {
  width:  24px;
  height: 24px;
  border-radius: 50%;
  object-fit: contain;
}

/* ── Itens ───────────────────────────────────────────────── */
.igb-item-wrap {
  width:         28px;
  height:        28px;
  border-radius: 3px;
  overflow:      hidden;
  border:        1px solid rgba(200, 155, 60, 0.18);
  flex-shrink:   0;
  background:    rgba(10, 14, 28, 0.8);
}
.igb-boots {
  border-color: rgba(200, 155, 60, 0.30);
  margin-left:  2px;
}
.igb-item {
  width:      100%;
  height:     100%;
  object-fit: cover;
  display:    block;
}

/* ── Skill order ─────────────────────────────────────────── */
.igb-skills {
  display:     flex;
  align-items: center;
  gap:         3px;
}
.igb-skill {
  font-family:    'Rajdhani', 'Inter', sans-serif;
  font-size:      .68rem;
  font-weight:    700;
  color:          #C8AA6E;
  background:     rgba(200, 155, 60, 0.07);
  border:         1px solid rgba(200, 155, 60, 0.22);
  border-radius:  2px;
  padding:        0 5px;
  line-height:    1.5;
  letter-spacing: .04em;
}

/* ── Badges ──────────────────────────────────────────────── */
.igb-badge-wrap {
  display:     flex;
  align-items: center;
  gap:         3px;
}
.igb-badge-role {
  font-family:    'Rajdhani', 'Inter', sans-serif;
  font-size:      .44rem;
  font-weight:    600;
  letter-spacing: .14em;
  text-transform: uppercase;
  color:          rgba(200, 155, 60, 0.40);
}
.igb-badge-sc {
  font-family:    'Rajdhani', 'Inter', sans-serif;
  font-size:      .42rem;
  font-weight:    700;
  letter-spacing: .12em;
  padding:        1px 4px;
  border-radius:  2px;
  background:     rgba(6, 182, 212, 0.10);
  color:          rgba(6, 182, 212, 0.60);
  border:         1px solid rgba(6, 182, 212, 0.20);
}

/* ── Estado colapsado ────────────────────────────────────── */
.igb-collapsed .igb-toggle {
  border-right:  1px solid rgba(200, 155, 60, 0.20);
  border-radius: 3px;
}

/* ── Transições ──────────────────────────────────────────── */
.igb-fade-enter-active { transition: opacity .3s ease, transform .3s ease; }
.igb-fade-leave-active { transition: opacity .2s ease, transform .2s ease; }
.igb-fade-enter-from   { opacity: 0; transform: translateX(-10px); }
.igb-fade-leave-to     { opacity: 0; transform: translateX(-10px); }

.igb-expand-enter-active { transition: opacity .2s ease, transform .2s ease; }
.igb-expand-leave-active { transition: opacity .15s ease, transform .15s ease; }
.igb-expand-enter-from   { opacity: 0; transform: translateX(-8px); }
.igb-expand-leave-to     { opacity: 0; transform: translateX(-8px); }
</style>
