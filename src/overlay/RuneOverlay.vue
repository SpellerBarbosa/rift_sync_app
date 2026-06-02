<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import type { PickRecommendation, ChampionInfo } from '../stores/champSelect'

const props = defineProps<{
  recommendation: PickRecommendation | null
  champion:       ChampionInfo | null
  isLoading:      boolean
  recError:       string | null
}>()

const emit = defineEmits<{ dismiss: [] }>()

const DD_BASE = 'https://ddragon.leagueoflegends.com/cdn'
const ddVer   = ref('16.10.1')

interface RuneEntry { id: number; key: string; icon: string; name: string }
interface SlotEntry { runes: RuneEntry[] }
interface PathEntry { id: number; key: string; icon: string; name: string; slots: SlotEntry[] }

const runeTree = ref<PathEntry[]>([])

async function loadRuneData() {
  if (runeTree.value.length) return
  try {
    const versRes  = await fetch('https://ddragon.leagueoflegends.com/api/versions.json')
    const versions: string[] = await versRes.json()
    ddVer.value = versions[0]
    const res = await fetch(`${DD_BASE}/${ddVer.value}/data/en_US/runesReforged.json`)
    runeTree.value = await res.json()
  } catch (e) {
    console.warn('[RuneOverlay] rune data fetch failed', e)
  }
}

onMounted(loadRuneData)
watch(() => props.recommendation, (r) => { if (r) loadRuneData() })

const pathById = computed(() => {
  const m = new Map<number, PathEntry>()
  for (const p of runeTree.value) m.set(p.id, p)
  return m
})

const primaryPath   = computed(() => pathById.value.get(props.recommendation?.primaryPathId ?? 0))
const secondaryPath = computed(() => pathById.value.get(props.recommendation?.secondaryPathId ?? 0))
const primarySlots  = computed(() => primaryPath.value?.slots ?? [])
const secondarySlots = computed(() => (secondaryPath.value?.slots ?? []).slice(1))

const primarySelected   = computed(() => new Set(props.recommendation?.primaryRuneIds ?? []))
const secondarySelected = computed(() => new Set(props.recommendation?.secondaryRuneIds ?? []))

function isSelectedPrimary(id: number)   { return primarySelected.value.has(id) }
function isSelectedSecondary(id: number) { return secondarySelected.value.has(id) }

function runeUrl(icon: string) { return `${DD_BASE}/img/${icon}` }
function pathUrl(icon: string) { return `${DD_BASE}/img/${icon}` }

const SPELL_KEYS: Record<number, string> = {
  1: 'SummonerBoost', 3: 'SummonerExhaust', 4: 'SummonerFlash',
  6: 'SummonerHaste', 7: 'SummonerHeal',   11: 'SummonerSmite',
  12: 'SummonerTeleport', 14: 'SummonerDot', 21: 'SummonerBarrier',
}
function spellUrl(id: number) {
  return `${DD_BASE}/${ddVer.value}/img/spell/${(SPELL_KEYS[id] ?? 'SummonerFlash')}.png`
}
function champUrl(key: string) {
  return `${DD_BASE}/${ddVer.value}/img/champion/${key}.png`
}

const SHARD_ICONS: Record<number, string> = {
  5008: 'perk-images/StatMods/StatModsAdaptiveForceIcon.png',
  5005: 'perk-images/StatMods/StatModsAttackSpeedIcon.png',
  5007: 'perk-images/StatMods/StatModsCDRScalingIcon.png',
  5002: 'perk-images/StatMods/StatModsArmorIcon.png',
  5003: 'perk-images/StatMods/StatModsMagicResIcon.png',
  5001: 'perk-images/StatMods/StatModsHealthScalingIcon.png',
}
const STAT_ROWS = [
  [5008, 5005, 5007],
  [5008, 5002, 5003],
  [5001, 5002, 5003],
]
function shardUrl(id: number) {
  return `${DD_BASE}/img/${SHARD_ICONS[id] ?? SHARD_ICONS[5008]}`
}

const ROLE_LABELS: Record<string, string> = {
  TOP: 'Top', JUNGLE: 'Jungle', MIDDLE: 'Mid', BOTTOM: 'Bot', UTILITY: 'Suporte',
}
function roleLabel(r: string) { return ROLE_LABELS[r] ?? r }

const PATH_COLORS: Record<string, string> = {
  Precision: '#C8AA6E', Domination: '#C0392B',
  Sorcery: '#7FC7FF', Resolve: '#27AE60', Inspiration: '#5BC8AF',
}
function pathColor(name: string) { return PATH_COLORS[name] ?? '#C89B3C' }
</script>

<template>
  <div class="rune-panel" @click.stop>

    <!-- Loading -->
    <div v-if="isLoading" class="state-wrap">
      <div class="skel skel-header" />
      <div class="skel skel-row" />
      <div class="skel skel-row-sm" />
      <div class="skel skel-row-sm" />
      <div class="skel skel-row-sm" />
    </div>

    <!-- Error -->
    <div v-else-if="recError" class="state-wrap">
      <button class="close-btn abs-close" @click="emit('dismiss')">✕</button>
      <p class="err-title">Falha ao carregar</p>
      <p class="err-msg">{{ recError }}</p>
    </div>

    <!-- Content -->
    <template v-else-if="recommendation && champion">

      <!-- Header -->
      <div class="header">
        <img :src="champUrl(champion.key)" class="champ-icon"
             @error="($event.target as HTMLImageElement).style.display='none'" />
        <div class="champ-meta">
          <span class="champ-name">{{ champion.name }}</span>
          <span class="role-tag">{{ roleLabel(recommendation.role) }}</span>
        </div>
        <div class="header-right">
          <span v-if="recommendation.metaWinRate !== null"
                class="wr-badge"
                :class="recommendation.metaWinRate >= 52 ? 'wr-good' : recommendation.metaWinRate < 48 ? 'wr-bad' : 'wr-ok'">
            {{ recommendation.metaWinRate.toFixed(1) }}% WR
          </span>
          <img :src="spellUrl(recommendation.spell1Id)" class="spell-icon" :title="recommendation.spell1Name"
               @error="($event.target as HTMLImageElement).style.opacity='.2'" />
          <img :src="spellUrl(recommendation.spell2Id)" class="spell-icon" :title="recommendation.spell2Name"
               @error="($event.target as HTMLImageElement).style.opacity='.2'" />
          <button class="close-btn" @click="emit('dismiss')">✕</button>
        </div>
      </div>

      <!-- Two-column rune body -->
      <div class="rune-body" v-if="primaryPath && secondaryPath">

        <!-- PRIMARY column -->
        <div class="path-col">
          <div class="path-label" :style="{ '--pc': pathColor(recommendation.primaryPath) }">
            <img :src="pathUrl(primaryPath.icon)" class="path-icon"
                 @error="($event.target as HTMLImageElement).style.visibility='hidden'" />
            <span>{{ recommendation.primaryPath }}</span>
          </div>

          <!-- Keystone -->
          <template v-if="primarySlots[0]">
            <template v-for="r in primarySlots[0].runes" :key="r.id">
              <div v-if="isSelectedPrimary(r.id)"
                   class="rune-row"
                   :style="{ '--pc': pathColor(recommendation.primaryPath) }">
                <div class="rune-orb keystone-orb">
                  <img :src="runeUrl(r.icon)" :alt="r.name" class="rune-img keystone-img"
                       @error="($event.target as HTMLImageElement).style.opacity='.15'" />
                </div>
                <span class="rune-name">{{ r.name }}</span>
              </div>
            </template>
          </template>

          <!-- Tier runes 1–3 -->
          <template v-for="(slot, si) in primarySlots.slice(1)" :key="si">
            <template v-for="r in slot.runes" :key="r.id">
              <div v-if="isSelectedPrimary(r.id)" class="rune-row tier-rune-row">
                <div class="rune-orb tier-orb">
                  <img :src="runeUrl(r.icon)" :alt="r.name" class="rune-img tier-img"
                       @error="($event.target as HTMLImageElement).style.opacity='.15'" />
                </div>
                <span class="rune-name">{{ r.name }}</span>
              </div>
            </template>
          </template>
        </div>

        <!-- Column divider -->
        <div class="col-divider" />

        <!-- SECONDARY column -->
        <div class="path-col">
          <div class="path-label" :style="{ '--pc': pathColor(recommendation.secondaryPath) }">
            <img :src="pathUrl(secondaryPath.icon)" class="path-icon"
                 @error="($event.target as HTMLImageElement).style.visibility='hidden'" />
            <span>{{ recommendation.secondaryPath }}</span>
          </div>

          <!-- 2 selected secondary runes -->
          <template v-for="(slot, si) in secondarySlots" :key="si">
            <template v-for="r in slot.runes" :key="r.id">
              <div v-if="isSelectedSecondary(r.id)" class="rune-row tier-rune-row">
                <div class="rune-orb tier-orb">
                  <img :src="runeUrl(r.icon)" :alt="r.name" class="rune-img tier-img"
                       @error="($event.target as HTMLImageElement).style.opacity='.15'" />
                </div>
                <span class="rune-name">{{ r.name }}</span>
              </div>
            </template>
          </template>

          <!-- Stat shards -->
          <div class="shards-section">
            <div v-for="(row, ri) in STAT_ROWS" :key="ri" class="shard-row">
              <div v-for="shardId in row" :key="shardId"
                   class="shard-orb"
                   :class="recommendation.statShardIds[ri] === shardId ? 'shard-on' : 'shard-off'">
                <img :src="shardUrl(shardId)" class="shard-img"
                     @error="($event.target as HTMLImageElement).style.opacity='.1'" />
              </div>
            </div>
          </div>
        </div>

      </div>

      <!-- Fallback: rune tree still loading -->
      <div v-else class="state-wrap">
        <div class="skel skel-row" />
        <div class="skel skel-row-sm" />
        <div class="skel skel-row-sm" />
      </div>

    </template>
  </div>
</template>

<style scoped>
/* ── Panel shell ─────────────────────────────────────────── */
.rune-panel {
  position: fixed;
  inset: 0;
  background: linear-gradient(160deg, #0d0e15 0%, #080910 100%);
  border: 1px solid rgba(200,155,60,.18);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  font-family: 'Rajdhani', 'Segoe UI', sans-serif;
  color: rgba(232,224,208,.9);
  pointer-events: auto;
}

/* ── Header ──────────────────────────────────────────────── */
.header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px 7px;
  border-bottom: 1px solid rgba(200,155,60,.13);
  flex-shrink: 0;
}
.champ-icon {
  width: 34px; height: 34px;
  border-radius: 4px;
  border: 1px solid rgba(200,155,60,.35);
  object-fit: cover;
  object-position: center top;
  flex-shrink: 0;
}
.champ-meta {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}
.champ-name {
  font-size: .8rem;
  font-weight: 700;
  color: #C8AA6E;
  line-height: 1.2;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.role-tag {
  font-size: .46rem;
  letter-spacing: .18em;
  text-transform: uppercase;
  color: rgba(200,155,60,.45);
}
.header-right {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.wr-badge {
  font-size: .54rem;
  font-weight: 700;
  letter-spacing: .03em;
  white-space: nowrap;
}
.wr-good { color: #27AE60; }
.wr-ok   { color: #C8AA6E; }
.wr-bad  { color: #C0392B; }
.spell-icon {
  width: 20px; height: 20px;
  border-radius: 3px;
  border: 1px solid rgba(200,155,60,.25);
  object-fit: cover;
}
.close-btn {
  background: none;
  border: none;
  color: rgba(232,224,208,.25);
  cursor: pointer;
  font-size: .7rem;
  padding: 3px 4px;
  line-height: 1;
  transition: color .15s;
}
.close-btn:hover { color: rgba(232,224,208,.7); }
.abs-close { margin-left: auto; }

/* ── Two-column body ─────────────────────────────────────── */
.rune-body {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.path-col {
  flex: 1;
  display: flex;
  flex-direction: column;
  padding: 10px 8px 10px;
  min-width: 0;
}

.col-divider {
  width: 1px;
  background: rgba(200,155,60,.1);
  flex-shrink: 0;
  margin: 10px 0;
}

/* ── Path label ──────────────────────────────────────────── */
.path-label {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: .48rem;
  font-weight: 700;
  letter-spacing: .16em;
  text-transform: uppercase;
  color: var(--pc, #C8AA6E);
  opacity: .75;
  margin-bottom: 9px;
  flex-shrink: 0;
}
.path-icon {
  width: 13px; height: 13px;
  object-fit: contain;
  filter: brightness(.9);
}

/* ── Rune rows ───────────────────────────────────────────── */
.rune-row {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-bottom: 8px;
  flex-shrink: 0;
}
.tier-rune-row { margin-bottom: 6px; }

/* ── Rune orbs ───────────────────────────────────────────── */
.rune-orb {
  flex-shrink: 0;
  border-radius: 50%;
  background: rgba(0,0,0,.45);
  display: flex;
  align-items: center;
  justify-content: center;
}

.keystone-orb {
  width: 42px; height: 42px;
  border: 2px solid var(--pc, #C8AA6E);
  box-shadow: 0 0 10px var(--pc, #C8AA6E), inset 0 0 8px rgba(0,0,0,.5);
}
.keystone-img { width: 33px; height: 33px; }

.tier-orb {
  width: 26px; height: 26px;
  border: 1.5px solid rgba(200,155,60,.28);
}
.tier-img { width: 20px; height: 20px; }

.rune-img {
  border-radius: 50%;
  object-fit: contain;
  display: block;
}

/* ── Rune name ───────────────────────────────────────────── */
.rune-name {
  font-size: .6rem;
  font-weight: 600;
  color: rgba(232,224,208,.85);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
  line-height: 1.3;
}

/* ── Stat shards ─────────────────────────────────────────── */
.shards-section {
  margin-top: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-top: 8px;
  border-top: 1px solid rgba(200,155,60,.1);
}
.shard-row { display: flex; gap: 5px; }
.shard-orb { border-radius: 50%; }
.shard-img { width: 15px; height: 15px; border-radius: 50%; object-fit: contain; display: block; }
.shard-on  .shard-img { filter: drop-shadow(0 0 4px rgba(200,155,60,.85)) brightness(1.15); }
.shard-off .shard-img { opacity: .18; filter: grayscale(.75); }

/* ── Loading / error states ──────────────────────────────── */
.state-wrap {
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex: 1;
  position: relative;
}
.skel {
  border-radius: 4px;
  background: rgba(200,155,60,.07);
  animation: sk-pulse 1.4s ease infinite;
}
.skel-header  { height: 38px; }
.skel-row     { height: 34px; }
.skel-row-sm  { height: 24px; }
@keyframes sk-pulse { 0%,100%{opacity:.6} 50%{opacity:.2} }
.err-title { font-size: .68rem; font-weight: 600; color: rgba(192,57,43,.85); letter-spacing: .1em; margin: 0; }
.err-msg   { font-family: monospace; font-size: .54rem; color: rgba(232,224,208,.3); word-break: break-all; margin: 4px 0 0; }
</style>
