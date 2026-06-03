<script setup lang="ts">
// ============================================================
// BuildOverlay — Build completa do campeão após lock-in
//
// Posição: lado esquerdo da tela, verticalmente centralizado.
// Exibe (dados da SpellCoach API via banco local):
//   · Keystone + caminho primário + caminho secundário + shards
//   · Itens iniciais · Core build (3 itens) · Botas
//   · Ordem de habilidades (Q > E > W)
//   · Badge de tier/role da fonte dos dados
// ============================================================

import { computed, ref, watch, onMounted } from 'vue'
import type { ChampionBuild } from '../stores/champSelect'

const props = defineProps<{
  build:      ChampionBuild | null
  champion:   { id: number; name: string; key: string; title: string } | null
  isLoading:  boolean
}>()

const emit = defineEmits<{ dismiss: [] }>()

// ── DDragon ──────────────────────────────────────────────────
const DD_BASE = 'https://ddragon.leagueoflegends.com/cdn'
const ddVer   = ref('16.10.1')

async function loadDdVersion() {
  try {
    const r = await fetch('https://ddragon.leagueoflegends.com/api/versions.json')
    const v: string[] = await r.json()
    ddVer.value = v[0]
  } catch { /* usa fallback */ }
}

onMounted(loadDdVersion)
watch(() => props.build, (b) => { if (b) loadDdVersion() })

function itemUrl(id: number)  { return `${DD_BASE}/${ddVer.value}/img/item/${id}.png` }
function champUrl(key: string) { return `${DD_BASE}/${ddVer.value}/img/champion/${key}.png` }

// ── Runas ────────────────────────────────────────────────────
interface RuneEntry  { id: number; key: string; icon: string; name: string }
interface SlotEntry  { runes: RuneEntry[] }
interface PathEntry  { id: number; key: string; icon: string; name: string; slots: SlotEntry[] }

const runeTree = ref<PathEntry[]>([])

async function loadRuneData() {
  if (runeTree.value.length) return
  try {
    const r = await fetch(`${DD_BASE}/${ddVer.value}/data/pt_BR/runesReforged.json`)
    runeTree.value = await r.json()
  } catch { /* continua sem runas */ }
}

watch(ddVer, loadRuneData, { immediate: true })

const pathById = computed(() => {
  const m = new Map<number, PathEntry>()
  for (const p of runeTree.value) m.set(p.id, p)
  return m
})

function runeIconById(perkId: number): string {
  for (const path of runeTree.value) {
    for (const slot of path.slots) {
      const r = slot.runes.find(r => r.id === perkId)
      if (r) return `${DD_BASE}/img/${r.icon}`
    }
  }
  return ''
}

function pathIconById(styleId: number): string {
  const p = pathById.value.get(styleId)
  return p ? `${DD_BASE}/img/${p.icon}` : ''
}

function pathNameById(styleId: number): string {
  return pathById.value.get(styleId)?.name ?? ''
}

// ── Computed: dados formatados ────────────────────────────────

const topKeystone = computed(() => {
  const ks = props.build?.runes?.keystones
  if (!ks?.length) return null
  return [...ks].sort((a, b) => b.frequency - a.frequency)[0]
})

const topPrimary = computed(() => {
  const pr = props.build?.runes?.primaryRunes
  if (!pr?.length) return null
  return [...pr].sort((a, b) => b.frequency - a.frequency)[0]
})

const topSecondary = computed(() => {
  const sr = props.build?.runes?.secondaryRunes
  if (!sr?.length) return null
  return [...sr].sort((a, b) => b.frequency - a.frequency)[0]
})

const topShards = computed(() => {
  const sh = props.build?.runes?.shards
  if (!sh?.length) return null
  return [...sh].sort((a, b) => b.frequency - a.frequency)[0]
})

const topStartingItems = computed(() => {
  const si = props.build?.items?.startingItems
  if (!si?.length) return null
  return [...si].sort((a, b) => b.frequency - a.frequency)[0]
})

const topCoreBuild = computed(() => {
  const cb = props.build?.items?.coreBuilds
  if (!cb?.length) return null
  return [...cb].sort((a, b) => b.frequency - a.frequency)[0]
})

const topBoots = computed(() => {
  const bt = props.build?.items?.boots
  if (!bt?.length) return null
  return [...bt].sort((a, b) => b.frequency - a.frequency)[0]
})

const topSkillOrder = computed(() => {
  const sp = props.build?.skills?.skillPriority
  if (!sp?.length) return null
  return [...sp].sort((a, b) => b.frequency - a.frequency)[0]
})

const hasBuild = computed(() =>
  !!(props.build?.runes || props.build?.items || props.build?.skills)
)

// Cor do caminho primário
const PATH_COLORS: Record<string, string> = {
  Precision: '#C8AA6E', Domination: '#C0392B',
  Sorcery: '#7FC7FF', Resolve: '#27AE60', Inspiration: '#5BC8AF',
}
function pathColor(styleId: number): string {
  const name = pathNameById(styleId)
  return PATH_COLORS[name] ?? '#C89B3C'
}

// Shard icons
const SHARD_ICONS: Record<number, string> = {
  5008: 'perk-images/StatMods/StatModsAdaptiveForceIcon.png',
  5005: 'perk-images/StatMods/StatModsAttackSpeedIcon.png',
  5007: 'perk-images/StatMods/StatModsCDRScalingIcon.png',
  5002: 'perk-images/StatMods/StatModsArmorIcon.png',
  5003: 'perk-images/StatMods/StatModsMagicResIcon.png',
  5001: 'perk-images/StatMods/StatModsHealthScalingIcon.png',
  5013: 'perk-images/StatMods/StatModsTenacityIcon.png',
}
function shardUrl(id: number) {
  return `${DD_BASE}/img/${SHARD_ICONS[id] ?? SHARD_ICONS[5008]}`
}

function pct(v: number) { return (v * 100).toFixed(0) + '%' }
</script>

<template>
  <Transition name="build-slide">
    <div v-if="champion" class="bo-root" @click.stop>

      <!-- ── Header ─────────────────────────────────────── -->
      <div class="bo-header">
        <img
          v-if="champion?.key"
          :src="champUrl(champion.key)"
          class="bo-champ-icon"
          @error="($event.target as HTMLImageElement).style.display='none'"
        />
        <div class="bo-champ-text">
          <span class="bo-champ-name">{{ champion?.name }}</span>
          <span v-if="build" class="bo-tier-badge">
            {{ build.role || '—' }} · {{ build.tier || '—' }}
          </span>
        </div>
        <button class="bo-close" @click="emit('dismiss')">✕</button>
      </div>

      <!-- ── Loading ────────────────────────────────────── -->
      <div v-if="isLoading && !hasBuild" class="bo-loading">
        <div class="bo-skel bo-skel-h" />
        <div class="bo-skel bo-skel-row" />
        <div class="bo-skel bo-skel-row" />
        <div class="bo-skel bo-skel-row" />
      </div>

      <!-- ── Sem dados no banco ─────────────────────────── -->
      <div v-else-if="!isLoading && !hasBuild" class="bo-no-data">
        <p class="bo-no-data-msg">Build não sincronizada.</p>
        <p class="bo-no-data-hint">Abra Configurações → Sincronizar agora.</p>
      </div>

      <template v-else-if="hasBuild">

        <!-- ── Keystone + caminhos ────────────────────────── -->
        <section v-if="topPrimary" class="bo-section">
          <span class="bo-section-label">Runas</span>
          <div class="bo-rune-row">
            <!-- Keystone -->
            <div v-if="topKeystone" class="bo-keystone-wrap"
                 :style="{ '--pc': pathColor(topPrimary.styleId) }">
              <img
                :src="runeIconById(topKeystone.perkId)"
                class="bo-keystone-icon"
                @error="($event.target as HTMLImageElement).style.opacity='.15'"
              />
            </div>

            <!-- Runas primárias (4 ícones menores) -->
            <div class="bo-rune-group">
              <img
                v-for="(runeId, i) in topPrimary.selections"
                :key="i"
                :src="runeIconById(runeId)"
                class="bo-rune-sm"
                @error="($event.target as HTMLImageElement).style.opacity='.15'"
              />
            </div>

            <div class="bo-path-divider" />

            <!-- Caminho secundário -->
            <div v-if="topSecondary" class="bo-rune-group">
              <img :src="pathIconById(topSecondary.styleId)" class="bo-path-icon-sm"
                   @error="($event.target as HTMLImageElement).style.opacity='.15'" />
              <img
                v-for="(runeId, i) in topSecondary.selections"
                :key="i"
                :src="runeIconById(runeId)"
                class="bo-rune-sm"
                @error="($event.target as HTMLImageElement).style.opacity='.15'"
              />
            </div>
          </div>

          <!-- Shards -->
          <div v-if="topShards" class="bo-shards-row">
            <img :src="shardUrl(topShards.offense)"  class="bo-shard" title="Ofensa" />
            <img :src="shardUrl(topShards.flex)"     class="bo-shard" title="Flex" />
            <img :src="shardUrl(topShards.defense)"  class="bo-shard" title="Defesa" />
            <span class="bo-winrate">{{ pct(topPrimary.winRate) }} WR</span>
          </div>
        </section>

        <!-- ── Itens iniciais ─────────────────────────────── -->
        <section v-if="topStartingItems" class="bo-section">
          <span class="bo-section-label">Início</span>
          <div class="bo-items-row">
            <div v-for="itemId in topStartingItems.items" :key="itemId" class="bo-item-wrap">
              <img
                :src="itemUrl(itemId)"
                class="bo-item-icon"
                @error="($event.target as HTMLImageElement).style.opacity='.15'"
              />
            </div>
            <span class="bo-winrate">{{ pct(topStartingItems.winRate) }} WR</span>
          </div>
        </section>

        <!-- ── Core build ─────────────────────────────────── -->
        <section v-if="topCoreBuild" class="bo-section">
          <span class="bo-section-label">Core</span>
          <div class="bo-items-row">
            <div v-for="itemId in topCoreBuild.items" :key="itemId" class="bo-item-wrap">
              <img
                :src="itemUrl(itemId)"
                class="bo-item-icon"
                @error="($event.target as HTMLImageElement).style.opacity='.15'"
              />
            </div>
            <div v-if="topBoots" class="bo-item-wrap bo-boots-sep">
              <img
                :src="itemUrl(topBoots.itemId)"
                class="bo-item-icon"
                @error="($event.target as HTMLImageElement).style.opacity='.15'"
                title="Botas recomendadas"
              />
            </div>
            <span class="bo-winrate">{{ pct(topCoreBuild.winRate) }} WR</span>
          </div>
        </section>

        <!-- ── Ordem de habilidades ───────────────────────── -->
        <section v-if="topSkillOrder" class="bo-section">
          <span class="bo-section-label">Skills</span>
          <div class="bo-skill-row">
            <span
              v-for="(skill, i) in topSkillOrder.order.split(' > ')"
              :key="i"
              class="bo-skill-chip"
            >{{ skill }}</span>
            <span class="bo-winrate">{{ pct(topSkillOrder.winRate) }} WR</span>
          </div>
        </section>

      </template>

    </div>
  </Transition>
</template>

<style scoped>
/* ── Painel: lado esquerdo, verticalmente centrado ─────────── */
.bo-root {
  position:       fixed;
  left:           14px;
  top:            50%;
  transform:      translateY(-50%);
  z-index:        30;
  pointer-events: auto;
  user-select:    none;
  width:          264px;

  background:     rgba(5, 10, 24, 0.97);
  border:         1px solid rgba(200, 155, 60, 0.22);
  border-radius:  4px;
  clip-path: polygon(
    0 0,
    calc(100% - 12px) 0,
    100% 12px,
    100% 100%,
    12px 100%,
    0 calc(100% - 12px)
  );
  display:        flex;
  flex-direction: column;
  gap:            0;
  overflow:       hidden;
  backdrop-filter: blur(12px);
}

/* ── Header ─────────────────────────────────────────────── */
.bo-header {
  display:     flex;
  align-items: center;
  gap:         8px;
  padding:     9px 10px 7px;
  border-bottom: 1px solid rgba(200,155,60,.12);
  flex-shrink: 0;
}
.bo-champ-icon {
  width:         32px;
  height:        32px;
  border-radius: 3px;
  object-fit:    cover;
  border:        1px solid rgba(200,155,60,.3);
  flex-shrink:   0;
}
.bo-champ-text {
  display:        flex;
  flex-direction: column;
  flex:           1;
  min-width:      0;
}
.bo-champ-name {
  font-family:    'Rajdhani','Inter',sans-serif;
  font-size:      .82rem;
  font-weight:    700;
  color:          #C8AA6E;
  line-height:    1.2;
}
.bo-tier-badge {
  font-family:    'Rajdhani','Inter',sans-serif;
  font-size:      .48rem;
  letter-spacing: .15em;
  text-transform: uppercase;
  color:          rgba(200,155,60,.4);
}
.bo-close {
  background: none;
  border:     none;
  color:      rgba(232,224,208,.25);
  cursor:     pointer;
  font-size:  .75rem;
  padding:    4px;
  line-height: 1;
  transition: color .15s;
  flex-shrink: 0;
}
.bo-close:hover { color: rgba(232,224,208,.65); }

/* ── Seções ──────────────────────────────────────────────── */
.bo-section {
  padding:       6px 10px;
  border-bottom: 1px solid rgba(200,155,60,.07);
  display:       flex;
  flex-direction: column;
  gap:           5px;
}
.bo-section:last-child { border-bottom: none; }

.bo-section-label {
  font-family:    'Rajdhani','Inter',sans-serif;
  font-size:      .48rem;
  font-weight:    700;
  letter-spacing: .22em;
  text-transform: uppercase;
  color:          rgba(200,155,60,.45);
}

/* ── Runas ───────────────────────────────────────────────── */
.bo-rune-row {
  display:     flex;
  align-items: center;
  gap:         6px;
}
.bo-keystone-wrap {
  width:         44px;
  height:        44px;
  border-radius: 50%;
  border:        2px solid var(--pc, #C8AA6E);
  display:       flex;
  align-items:   center;
  justify-content: center;
  flex-shrink:   0;
  background:    rgba(0,0,0,.3);
}
.bo-keystone-icon {
  width:   36px;
  height:  36px;
  border-radius: 50%;
  object-fit: contain;
}
.bo-rune-group {
  display:     flex;
  align-items: center;
  gap:         3px;
  flex-wrap:   wrap;
}
.bo-rune-sm {
  width:         22px;
  height:        22px;
  border-radius: 50%;
  object-fit:    contain;
  opacity:       .9;
}
.bo-path-divider {
  width:      1px;
  height:     28px;
  background: rgba(200,155,60,.15);
  flex-shrink: 0;
}
.bo-path-icon-sm {
  width:      16px;
  height:     16px;
  object-fit: contain;
  opacity:    .7;
}
.bo-shards-row {
  display:     flex;
  align-items: center;
  gap:         4px;
}
.bo-shard {
  width:         18px;
  height:        18px;
  border-radius: 50%;
  object-fit:    contain;
  opacity:       .8;
}

/* ── Itens ───────────────────────────────────────────────── */
.bo-items-row {
  display:     flex;
  align-items: center;
  gap:         4px;
  flex-wrap:   wrap;
}
.bo-item-wrap {
  width:         34px;
  height:        34px;
  border-radius: 4px;
  overflow:      hidden;
  border:        1px solid rgba(200,155,60,.2);
  flex-shrink:   0;
  background:    rgba(10,14,28,.8);
}
.bo-boots-sep {
  margin-left: 4px;
  border-color: rgba(200,155,60,.35);
}
.bo-item-icon {
  width:      100%;
  height:     100%;
  object-fit: cover;
  display:    block;
}

/* ── Skills ──────────────────────────────────────────────── */
.bo-skill-row {
  display:     flex;
  align-items: center;
  gap:         4px;
  flex-wrap:   wrap;
}
.bo-skill-chip {
  font-family:    'Rajdhani','Inter',sans-serif;
  font-size:      .75rem;
  font-weight:    700;
  letter-spacing: .06em;
  color:          #C8AA6E;
  background:     rgba(200,155,60,.08);
  border:         1px solid rgba(200,155,60,.25);
  border-radius:  3px;
  padding:        1px 7px;
  line-height:    1.6;
}

/* ── Win rate tag ────────────────────────────────────────── */
.bo-winrate {
  font-family:    'Rajdhani','Inter',sans-serif;
  font-size:      .52rem;
  font-weight:    600;
  color:          rgba(39,174,96,.8);
  margin-left:    auto;
  flex-shrink:    0;
}

/* ── Sem dados ───────────────────────────────────────────── */
.bo-no-data {
  padding:         16px 12px;
  display:         flex;
  flex-direction:  column;
  gap:             4px;
  align-items:     flex-start;
}
.bo-no-data-msg {
  font-family:    'Rajdhani', 'Inter', sans-serif;
  font-size:      .72rem;
  font-weight:    600;
  color:          rgba(200,155,60,.5);
  letter-spacing: .06em;
}
.bo-no-data-hint {
  font-family:    'Rajdhani', 'Inter', sans-serif;
  font-size:      .62rem;
  color:          rgba(232,224,208,.25);
  letter-spacing: .04em;
}

/* ── Skeleton ────────────────────────────────────────────── */
.bo-loading {
  display:        flex;
  flex-direction: column;
  gap:            8px;
  padding:        10px;
}
.bo-skel {
  border-radius: 3px;
  background:    rgba(200,155,60,.06);
  animation:     bo-pulse 1.4s ease infinite;
}
.bo-skel-h   { height: 36px; }
.bo-skel-row { height: 24px; }
@keyframes bo-pulse { 0%,100%{opacity:.5} 50%{opacity:.15} }

/* ── Transição: slide da esquerda ────────────────────────── */
.build-slide-enter-active { transition: all .32s cubic-bezier(.25,.46,.45,.94); }
.build-slide-leave-active { transition: all .22s ease-in; }
.build-slide-enter-from   { opacity: 0; transform: translateY(-50%) translateX(-18px); }
.build-slide-leave-to     { opacity: 0; transform: translateY(-50%) translateX(-12px); }
</style>
