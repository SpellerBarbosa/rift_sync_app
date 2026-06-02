<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

interface LoadingPlayer {
  summonerName: string
  championId: number
  championName: string
  championKey: string
  position: string
  team: number
  isLocal: boolean
  spell1Id: number
  spell2Id: number
  tags: string[]
  adPct: number
  apPct: number
  truePct: number
}

const players = ref<LoadingPlayer[]>([])
const loading = ref(true)

const SPELL_NAMES: Record<number, string> = {
  1: 'SummonerBoost',
  3: 'SummonerExhaust',
  4: 'SummonerFlash',
  6: 'SummonerHaste',
  7: 'SummonerHeal',
  11: 'SummonerSmite',
  12: 'SummonerTeleport',
  13: 'SummonerMana',
  14: 'SummonerDot',
  21: 'SummonerBarrier',
  32: 'SummonerSnowball',
}

const ddVersion = ref('14.24.1')

async function fetchDdVersion() {
  try {
    const res = await fetch('https://ddragon.leagueoflegends.com/api/versions.json')
    const versions = await res.json()
    ddVersion.value = versions[0]
  } catch {
    ddVersion.value = '14.24.1'
  }
}

function splashUrl(key: string) {
  if (!key) return ''
  return `https://ddragon.leagueoflegends.com/cdn/img/champion/splash/${key}_0.jpg`
}

function spellUrl(spellId: number) {
  const name = SPELL_NAMES[spellId] ?? 'SummonerFlash'
  return `https://ddragon.leagueoflegends.com/cdn/${ddVersion.value}/img/spell/${name}.png`
}

function positionLabel(pos: string) {
  const map: Record<string, string> = {
    TOP: 'Top', JUNGLE: 'Jg', MIDDLE: 'Mid', BOTTOM: 'Bot', UTILITY: 'Sup'
  }
  return map[pos] ?? pos
}

const allies = ref<LoadingPlayer[]>([])
const enemies = ref<LoadingPlayer[]>([])

onMounted(async () => {
  await fetchDdVersion()
  try {
    const data = await invoke<LoadingPlayer[]>('get_loading_screen_players')
    players.value = data
    allies.value  = data.filter(p => p.team === 1)
    enemies.value = data.filter(p => p.team === 2)
  } catch (e) {
    console.error('LoadingOverlay:', e)
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="loading-screen">
    <div v-if="loading" class="loading-spinner">
      <div class="spinner" />
    </div>

    <template v-else>
      <!-- Allies -->
      <div class="team-row team-blue">
        <div
          v-for="p in allies"
          :key="p.summonerName"
          class="player-card"
          :class="{ 'is-local': p.isLocal }"
        >
          <div
            class="splash-bg"
            :style="p.championKey ? `background-image: url('${splashUrl(p.championKey)}')` : ''"
          />
          <div class="card-gradient" />

          <div class="card-content">
            <div class="position-badge">{{ positionLabel(p.position) }}</div>

            <div class="spells">
              <img
                v-if="p.spell1Id"
                :src="spellUrl(p.spell1Id)"
                class="spell-icon"
                @error="($event.target as HTMLImageElement).style.display='none'"
              />
              <img
                v-if="p.spell2Id"
                :src="spellUrl(p.spell2Id)"
                class="spell-icon"
                @error="($event.target as HTMLImageElement).style.display='none'"
              />
            </div>

            <div class="player-info">
              <div class="champion-name">{{ p.championName || '—' }}</div>
              <div class="summoner-name" :class="{ 'local-highlight': p.isLocal }">
                {{ p.summonerName }}
              </div>
            </div>

            <div class="tags">
              <span v-for="tag in p.tags.slice(0, 3)" :key="tag" class="tag">{{ tag }}</span>
            </div>

            <div class="damage-bar">
              <div
                v-if="p.adPct > 0"
                class="bar-seg ad"
                :style="{ width: p.adPct + '%' }"
                :title="`${p.adPct}% AD`"
              />
              <div
                v-if="p.apPct > 0"
                class="bar-seg ap"
                :style="{ width: p.apPct + '%' }"
                :title="`${p.apPct}% AP`"
              />
              <div
                v-if="p.truePct > 0"
                class="bar-seg true-dmg"
                :style="{ width: p.truePct + '%' }"
                :title="`${p.truePct}% True`"
              />
            </div>
            <div class="bar-labels">
              <span v-if="p.adPct > 0" class="ad-lbl">{{ p.adPct }}% AD</span>
              <span v-if="p.apPct > 0" class="ap-lbl">{{ p.apPct }}% AP</span>
              <span v-if="p.truePct > 0" class="true-lbl">{{ p.truePct }}% True</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Divider -->
      <div class="team-divider">
        <span class="vs-badge">VS</span>
      </div>

      <!-- Enemies -->
      <div class="team-row team-red">
        <div
          v-for="p in enemies"
          :key="p.summonerName"
          class="player-card enemy-card"
          :class="{ 'is-local': p.isLocal }"
        >
          <div
            class="splash-bg"
            :style="p.championKey ? `background-image: url('${splashUrl(p.championKey)}')` : ''"
          />
          <div class="card-gradient" />

          <div class="card-content">
            <div class="position-badge">{{ positionLabel(p.position) }}</div>

            <div class="spells">
              <img
                v-if="p.spell1Id"
                :src="spellUrl(p.spell1Id)"
                class="spell-icon"
                @error="($event.target as HTMLImageElement).style.display='none'"
              />
              <img
                v-if="p.spell2Id"
                :src="spellUrl(p.spell2Id)"
                class="spell-icon"
                @error="($event.target as HTMLImageElement).style.display='none'"
              />
            </div>

            <div class="player-info">
              <div class="champion-name">{{ p.championName || '—' }}</div>
              <div class="summoner-name" :class="{ 'local-highlight': p.isLocal }">
                {{ p.summonerName }}
              </div>
            </div>

            <div class="tags">
              <span v-for="tag in p.tags.slice(0, 3)" :key="tag" class="tag enemy-tag">{{ tag }}</span>
            </div>

            <div class="damage-bar">
              <div
                v-if="p.adPct > 0"
                class="bar-seg ad"
                :style="{ width: p.adPct + '%' }"
              />
              <div
                v-if="p.apPct > 0"
                class="bar-seg ap"
                :style="{ width: p.apPct + '%' }"
              />
              <div
                v-if="p.truePct > 0"
                class="bar-seg true-dmg"
                :style="{ width: p.truePct + '%' }"
              />
            </div>
            <div class="bar-labels">
              <span v-if="p.adPct > 0" class="ad-lbl">{{ p.adPct }}% AD</span>
              <span v-if="p.apPct > 0" class="ap-lbl">{{ p.apPct }}% AP</span>
              <span v-if="p.truePct > 0" class="true-lbl">{{ p.truePct }}% True</span>
            </div>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.loading-screen {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  background: #0a0a0f;
  overflow: hidden;
  font-family: 'Beaufort for LOL', 'Segoe UI', sans-serif;
}

/* ── Loading spinner ──────────────────────────────── */
.loading-spinner {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.spinner {
  width: 48px;
  height: 48px;
  border: 4px solid rgba(200, 170, 110, 0.2);
  border-top-color: #c8aa6e;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

/* ── Team rows ────────────────────────────────────── */
.team-row {
  flex: 1;
  display: flex;
  flex-direction: row;
  min-height: 0;
}

.team-divider {
  height: 28px;
  background: linear-gradient(90deg, #0d1b3e 0%, #1a0a0a 100%);
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  flex-shrink: 0;
}
.vs-badge {
  background: #1a1a2e;
  border: 1px solid #c8aa6e55;
  color: #c8aa6e;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 2px;
  padding: 2px 10px;
  border-radius: 12px;
}

/* ── Player card ──────────────────────────────────── */
.player-card {
  flex: 1;
  position: relative;
  overflow: hidden;
  border-right: 1px solid rgba(255,255,255,0.05);
  cursor: default;
  min-width: 0;
}
.player-card:last-child { border-right: none; }

.player-card.is-local::after {
  content: '';
  position: absolute;
  inset: 0;
  border: 2px solid #c8aa6e;
  pointer-events: none;
  z-index: 10;
}

.splash-bg {
  position: absolute;
  inset: 0;
  background-size: cover;
  background-position: center top;
  background-color: #111;
  transition: transform 0.3s ease;
}
.player-card:hover .splash-bg {
  transform: scale(1.04);
}

/* Ally gradient: bottom dark, top-right fade to blue */
.team-blue .card-gradient {
  position: absolute;
  inset: 0;
  background: linear-gradient(
    180deg,
    rgba(5, 12, 38, 0.25) 0%,
    rgba(5, 12, 38, 0.55) 45%,
    rgba(5, 12, 38, 0.92) 100%
  );
}

/* Enemy gradient: same but reddish */
.team-red .card-gradient {
  position: absolute;
  inset: 0;
  background: linear-gradient(
    180deg,
    rgba(38, 5, 5, 0.25) 0%,
    rgba(38, 5, 5, 0.55) 45%,
    rgba(38, 5, 5, 0.92) 100%
  );
}

/* ── Card content ──────────────────────────────────── */
.card-content {
  position: relative;
  z-index: 2;
  height: 100%;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  padding: 6px 8px 8px;
  gap: 3px;
}

.position-badge {
  position: absolute;
  top: 6px;
  left: 6px;
  background: rgba(0,0,0,0.65);
  border: 1px solid rgba(200,170,110,0.4);
  color: #c8aa6e;
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 1px;
  padding: 1px 5px;
  border-radius: 3px;
  text-transform: uppercase;
}

.spells {
  position: absolute;
  top: 6px;
  right: 6px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.spell-icon {
  width: 18px;
  height: 18px;
  border-radius: 3px;
  border: 1px solid rgba(200,170,110,0.3);
}

/* ── Player info ─────────────────────────────────── */
.player-info {
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.champion-name {
  color: #c8aa6e;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.5px;
  text-transform: uppercase;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.summoner-name {
  color: rgba(255,255,255,0.85);
  font-size: 10px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.summoner-name.local-highlight {
  color: #c8aa6e;
  font-weight: 700;
}

/* ── Tags ────────────────────────────────────────── */
.tags {
  display: flex;
  flex-wrap: wrap;
  gap: 2px;
}
.tag {
  font-size: 8px;
  padding: 1px 5px;
  border-radius: 10px;
  border: 1px solid rgba(200,170,110,0.4);
  background: rgba(200,170,110,0.1);
  color: #c8aa6ecc;
  white-space: nowrap;
  letter-spacing: 0.3px;
}
.enemy-tag {
  border-color: rgba(192, 57, 43, 0.5);
  background: rgba(192, 57, 43, 0.12);
  color: rgba(255, 120, 100, 0.9);
}

/* ── Damage bar ──────────────────────────────────── */
.damage-bar {
  display: flex;
  height: 4px;
  border-radius: 2px;
  overflow: hidden;
  background: rgba(255,255,255,0.08);
}
.bar-seg { transition: width 0.5s ease; }
.bar-seg.ad       { background: #e07c3e; }
.bar-seg.ap       { background: #7fc7ff; }
.bar-seg.true-dmg { background: #e0e0e0; }

.bar-labels {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.ad-lbl   { color: #e07c3e; font-size: 7px; }
.ap-lbl   { color: #7fc7ff; font-size: 7px; }
.true-lbl { color: #bbb;    font-size: 7px; }
</style>
