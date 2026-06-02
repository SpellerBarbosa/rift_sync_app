# 🎮 RiftSync AI — Blueprint Completo de Desenvolvimento

> **Versão:** 2.0  
> **Stack:** Tauri + Vue 3 + TailwindCSS v4 + Rust + SQLite  
> **Filosofia:** Informação estratégica contextual em tempo real. Nunca automatização.

---

## 📌 Índice

1. [Visão Geral do Produto](#1-visão-geral-do-produto)
2. [Identidade Visual e UX](#2-identidade-visual-e-ux)
3. [Arquitetura Técnica](#3-arquitetura-técnica)
4. [APIs e Integrações Externas](#4-apis-e-integrações-externas)
5. [Estrutura de Pastas](#5-estrutura-de-pastas)
6. [Banco de Dados SQLite](#6-banco-de-dados-sqlite)
7. [Fases de Desenvolvimento](#7-fases-de-desenvolvimento)
   - [Fase 1 — Core Foundation](#fase-1--core-foundation)
   - [Fase 2 — Game State Engine](#fase-2--game-state-engine)
   - [Fase 3 — Overlay System](#fase-3--overlay-system)
   - [Fase 4 — Ban & Pick Assistant](#fase-4--ban--pick-assistant)
   - [Fase 5 — OCR + Visão Computacional](#fase-5--ocr--visão-computacional)
   - [Fase 6 — Coach em Tempo Real (TTS)](#fase-6--coach-em-tempo-real-tts)
   - [Fase 7 — Smart Vision System (Wards)](#fase-7--smart-vision-system-wards)
   - [Fase 8 — Pós-Game Review](#fase-8--pós-game-review)
   - [Fase 9 — Perfil Inteligente e Comportamental](#fase-9--perfil-inteligente-e-comportamental)
   - [Fase 10 — Performance e Otimização](#fase-10--performance-e-otimização)
   - [Fase 11 — Segurança e Compliance Riot](#fase-11--segurança-e-compliance-riot)
8. [Roadmap Resumido](#8-roadmap-resumido)
9. [Decisões de Arquitetura e Boas Práticas](#9-decisões-de-arquitetura-e-boas-práticas)

---

## 1. Visão Geral do Produto

O **RiftSync AI** é um aplicativo desktop nativo para Windows voltado a jogadores de League of Legends que desejam evoluir estrategicamente. Ele funciona como um **coach pessoal invisível**, presente em todas as fases da partida.

### Pilares Funcionais

| Pilar | Descrição |
|---|---|
| 🧠 Coaching Inteligente | Feedback contextual gerado por IA baseado no estado atual da partida |
| 👁️ Leitura de Tela | OCR + visão computacional para capturar informações sem acesso à memória do jogo |
| 🔊 Voz em Tempo Real | TTS com alertas de áudio inteligentes e não-intrusivos |
| 📊 Análise Pós-Game | Revisão detalhada de erros, padrões e pontos de melhoria |
| 🧬 Perfil Comportamental | Aprendizado contínuo sobre o estilo, vícios e evolução do jogador |
| 🗺️ Visão Estratégica | Recomendações de visão, macro e timing baseadas em dados reais |

### O que o RiftSync AI **NÃO faz**

- ❌ Mover o mouse automaticamente
- ❌ Executar ações no jogo
- ❌ Ler diretamente a memória do processo do League of Legends
- ❌ Injetar código no cliente
- ❌ Automatizar qualquer interação

---

## 2. Identidade Visual e UX

### Estilo Visual

- **Paleta principal:** Preto fosco (`#0A0A0B`), dourado LoL (`#C89B3C`), branco suave (`#E8E0D0`)
- **Paleta de suporte:** Azul mineral (`#1C2A3A`), vermelho alerta (`#C0392B`), verde sucesso (`#27AE60`)
- **Tipografia:** `Inter` para UI, `Rajdhani` ou `Beaufort for LOL` para títulos/overlays
- **Efeitos:** Blur sutil (`backdrop-filter: blur(8px)`), glow dourado discreto em elementos ativos, bordas com `1px solid rgba(200, 155, 60, 0.3)`

### Referências de Design

- **Blitz App** — estrutura de informação densa mas legível
- **Itero / Mobafire** — simplicidade de navegação
- **Interface do LoL** — linguagem visual familiar ao jogador
- **Absol.lol** — overlay minimalista

### Princípios de UX

1. **Zero distração durante a partida** — overlays devem ser consultados, não impostos
2. **Click-through por padrão** — o overlay não captura mouse/teclado a menos que necessário
3. **Feedback progressivo** — informação aparece quando relevante, some quando não é mais útil
4. **Acessibilidade sonora** — voz clara, objetiva e com volume configurável
5. **Modo silencioso** — tudo pode ser desligado com um atalho de teclado global

---

## 3. Arquitetura Técnica

```
┌─────────────────────────────────────────────────────────┐
│                    FRONTEND (Vue 3)                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐ │
│  │  Main UI │  │ Overlays │  │  Stores  │  │ Router │ │
│  │ (Tauri)  │  │ (Tauri)  │  │  (Pinia) │  │(Vue)   │ │
│  └──────────┘  └──────────┘  └──────────┘  └────────┘ │
└──────────────────────────┬──────────────────────────────┘
                           │ Tauri Commands / Events
┌──────────────────────────▼──────────────────────────────┐
│                     BACKEND (Rust)                      │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │  LCU Client │  │ Game State   │  │  OCR Engine   │  │
│  │  (HTTP/WS)  │  │  Manager     │  │  (Tesseract)  │  │
│  └─────────────┘  └──────────────┘  └───────────────┘  │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │  SQLite DB  │  │  TTS Client  │  │  Coach Engine │  │
│  │  (rusqlite) │  │  (HTTP)      │  │  (AI API)     │  │
│  └─────────────┘  └──────────────┘  └───────────────┘  │
└──────────────────────────┬──────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────┐
│                   EXTERNAL SERVICES                     │
│  Riot LCU API │ Riot Data Dragon │ SpellCoach API │ TTS │
└─────────────────────────────────────────────────────────┘
```

### Comunicação Frontend ↔ Backend

- **Tauri Commands**: chamadas síncronas/assíncronas do Vue para Rust (ex: buscar dados de campeão)
- **Tauri Events**: eventos assíncronos do Rust para o Vue (ex: `game_state_changed`, `coach_alert`)
- **Pinia Stores**: estado global reativo no frontend, sincronizado com eventos Tauri

---

## 4. APIs e Integrações Externas

### Riot LCU API (Local Client Update)

Acessada via `https://127.0.0.1:{port}` com certificado auto-assinado da Riot.

| Endpoint | Uso |
|---|---|
| `GET /lol-summoner/v1/current-summoner` | Dados do jogador logado |
| `GET /lol-champ-select/v1/session` | Estado atual da seleção de campeões |
| `GET /lol-lobby/v2/lobby` | Dados do lobby atual |
| `GET /lol-gameflow/v1/gameflow-phase` | Fase atual do client |
| `GET /lol-match-history/v1/products/lol/{puuid}/matches` | Histórico de partidas |
| `WebSocket /lol-game-data/...` | Eventos em tempo real |

> **Como detectar a porta e token:** ler o arquivo `lockfile` gerado pelo cliente na pasta de instalação do LoL.  
> Localização padrão: `C:\Riot Games\League of Legends\lockfile`  
> Formato: `LeagueClient:PID:PORT:TOKEN:PROTOCOL`

### Riot Data Dragon

- URL base: `https://ddragon.leagueoflegends.com/cdn/{version}/`
- Usos: ícones de campeões, itens, runas, spells, splash arts
- Cache local obrigatório para evitar latência em runtime

### SpellCoach API (própria)

- Base URL: `https://vercel.com/spell-0249b7c7/spellcoachapiv2`
- Responsável por: recomendações de runas, builds, matchups, tier lists

### TTS API

- URL: `https://spell2014-riftsyncai.hf.space/tts`
- Método: `POST` com payload `{ text: string, voice?: string }`
- Resposta: audio stream (MP3 ou WAV)
- Considerações: implementar fila de áudio para evitar sobreposição de falas

---

## 5. Estrutura de Pastas

```
riftsync-ai/
├── src-tauri/                    # Backend Rust
│   ├── src/
│   │   ├── main.rs               # Entry point Tauri
│   │   ├── commands/             # Tauri commands expostos ao frontend
│   │   │   ├── champion.rs
│   │   │   ├── coach.rs
│   │   │   ├── game_state.rs
│   │   │   └── player.rs
│   │   ├── lcu/                  # Cliente da LCU API
│   │   │   ├── client.rs         # HTTP client com cert bypass
│   │   │   ├── lockfile.rs       # Parser do lockfile
│   │   │   └── websocket.rs      # WebSocket listener
│   │   ├── game_state/           # Máquina de estados da partida
│   │   │   ├── manager.rs
│   │   │   └── states.rs
│   │   ├── ocr/                  # Motor de OCR
│   │   │   ├── engine.rs
│   │   │   ├── regions.rs        # Regiões de captura de tela
│   │   │   └── parser.rs
│   │   ├── tts/                  # Cliente TTS
│   │   │   ├── client.rs
│   │   │   └── queue.rs          # Fila de áudio
│   │   ├── coach/                # Lógica de coaching
│   │   │   ├── engine.rs
│   │   │   ├── roles/            # Coaches por role
│   │   │   └── alerts.rs
│   │   └── db/                   # Banco de dados
│   │       ├── connection.rs
│   │       ├── migrations.rs
│   │       └── models.rs
│   └── Cargo.toml
│
├── src/                          # Frontend Vue 3
│   ├── main.ts
│   ├── App.vue
│   ├── router/
│   │   └── index.ts
│   ├── stores/                   # Pinia stores
│   │   ├── gameState.ts
│   │   ├── champion.ts
│   │   ├── player.ts
│   │   └── coach.ts
│   ├── services/                 # Chamadas às Tauri commands
│   │   ├── lcu.ts
│   │   ├── champion.ts
│   │   └── coach.ts
│   ├── components/
│   │   ├── ui/                   # Componentes base (Button, Card, Badge...)
│   │   ├── champion/             # ChampionIcon, RuneDisplay, MatchupCard...
│   │   ├── coach/                # AlertToast, CoachPanel, VoiceToggle...
│   │   └── stats/                # KDAChart, WinrateBar, TimelineChart...
│   ├── pages/
│   │   ├── Dashboard.vue
│   │   ├── ChampSelect.vue
│   │   ├── InGame.vue
│   │   ├── PostGame.vue
│   │   └── Profile.vue
│   └── overlay/                  # Janelas de overlay separadas (Tauri multiwindow)
│       ├── RuneOverlay.vue
│       ├── MatchupOverlay.vue
│       ├── WardOverlay.vue
│       └── AlertOverlay.vue
│
├── tauri.conf.json
├── package.json
└── vite.config.ts
```

---

## 6. Banco de Dados SQLite

### Tabelas

```sql
-- Jogadores monitorados
CREATE TABLE players (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  puuid        TEXT UNIQUE NOT NULL,
  riot_name    TEXT NOT NULL,
  tag          TEXT NOT NULL,
  region       TEXT NOT NULL DEFAULT 'BR1',
  role         TEXT,           -- TOP | JGL | MID | ADC | SUP
  rank         TEXT,           -- IRON | BRONZE | ... | CHALLENGER
  lp           INTEGER,
  winrate      REAL,
  created_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at   DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Partidas registradas
CREATE TABLE matches (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  player_id       INTEGER REFERENCES players(id),
  match_id        TEXT UNIQUE NOT NULL,
  champion_id     INTEGER NOT NULL,
  champion_name   TEXT NOT NULL,
  role            TEXT,
  result          TEXT NOT NULL,   -- WIN | LOSS | REMAKE
  kills           INTEGER,
  deaths          INTEGER,
  assists         INTEGER,
  cs              INTEGER,
  cs_per_min      REAL,
  vision_score    INTEGER,
  damage_dealt    INTEGER,
  gold_earned     INTEGER,
  duration        INTEGER,         -- em segundos
  played_at       DATETIME
);

-- Padrões comportamentais do jogador
CREATE TABLE player_patterns (
  id                    INTEGER PRIMARY KEY AUTOINCREMENT,
  player_id             INTEGER REFERENCES players(id),
  aggression_score      REAL DEFAULT 0.5,   -- 0 = passivo, 1 = agressivo
  deaths_without_vision INTEGER DEFAULT 0,
  avg_deaths_10_15      REAL DEFAULT 0,      -- mortes entre min 10-15
  lane_dominance        REAL DEFAULT 0.5,
  objective_control     REAL DEFAULT 0.5,
  roam_frequency        REAL DEFAULT 0.5,
  tp_efficiency         REAL DEFAULT 0.5,
  ward_score            REAL DEFAULT 0.5,
  updated_at            DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Sessões de coaching (alertas e feedbacks)
CREATE TABLE coaching_sessions (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  match_id    TEXT REFERENCES matches(match_id),
  timestamp   INTEGER NOT NULL,   -- segundo da partida
  category    TEXT NOT NULL,      -- VISION | MACRO | TRADE | OBJECTIVE | POSITIONING
  severity    TEXT NOT NULL,      -- INFO | WARNING | CRITICAL
  message     TEXT NOT NULL,
  was_heard   BOOLEAN DEFAULT FALSE
);

-- Cache de dados do Data Dragon
CREATE TABLE champion_cache (
  champion_id   INTEGER PRIMARY KEY,
  name          TEXT NOT NULL,
  key           TEXT NOT NULL,
  data_json     TEXT NOT NULL,    -- JSON completo do campeão
  cached_at     DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Configurações do usuário
CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

---

## 7. Fases de Desenvolvimento

---

### FASE 1 — Core Foundation

**Objetivo:** Criar a base do projeto: estrutura, banco, autenticação local e conexão com o cliente LoL.

---

#### Passo 1 — Setup Inicial

```bash
# Criar projeto Tauri + Vue
npm create tauri-app@latest riftsync-ai -- --template vue-ts

# Instalar dependências frontend
npm install @tauri-apps/api pinia vue-router
npm install -D tailwindcss @tailwindcss/vite

# Dependências Rust (Cargo.toml)
# tauri, serde, serde_json, rusqlite, reqwest, tokio, anyhow
```

**Checklist:**
- [ ] Projeto Tauri inicializado e compilando
- [ ] Vue Router configurado com rotas base
- [ ] Pinia store de estado global criada
- [ ] TailwindCSS v4 funcionando
- [ ] Build de desenvolvimento estável

---

#### Passo 2 — Banco SQLite

- Usar `rusqlite` com migrations automáticas na inicialização
- Criar módulo `db/migrations.rs` que roda todas as migrations em ordem
- Expor commands Tauri: `get_player`, `save_match`, `get_patterns`

**Checklist:**
- [ ] Banco criado em `AppData/RiftSync/riftsync.db`
- [ ] Todas as tabelas criadas via migration
- [ ] CRUD básico funcionando via Tauri commands
- [ ] Dados persistindo entre sessões

---

#### Passo 3 — Integração LCU API

```rust
// src-tauri/src/lcu/lockfile.rs
pub struct LockfileData {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    pub protocol: String,
}

pub fn read_lockfile() -> Result<LockfileData> {
    // Ler de C:\Riot Games\League of Legends\lockfile
    // Parsear: name:pid:port:token:protocol
}
```

- Implementar polling do lockfile a cada 2 segundos para detectar abertura/fechamento do LoL
- Usar `reqwest` com `danger_accept_invalid_certs(true)` para o certificado autoassinado da Riot
- Autenticação: Basic Auth com `riot:{token}`

**Checklist:**
- [ ] Lockfile detectado automaticamente
- [ ] HTTP client configurado com Basic Auth e SSL bypass
- [ ] Endpoint `/lol-summoner/v1/current-summoner` funcionando
- [ ] WebSocket conectado ao LCU
- [ ] Evento de abertura/fechamento do LoL funcionando

---

### FASE 2 — Game State Engine

**Objetivo:** Saber exatamente em que fase do jogo o jogador está, em tempo real.

---

#### Passo 4 — Game State Manager

```typescript
// src/stores/gameState.ts
type GamePhase = 
  | 'IDLE'           // LoL fechado ou na tela inicial
  | 'LOBBY'          // Dentro de um lobby
  | 'MATCHMAKING'    // Na fila
  | 'MATCH_FOUND'    // Tela de aceitar partida
  | 'BAN_PHASE'      // Fase de banimento
  | 'PICK_PHASE'     // Fase de seleção
  | 'LOADING'        // Tela de carregamento
  | 'INGAME'         // Partida em andamento
  | 'POSTGAME'       // Tela de resultados
```

```rust
// Mapeamento do endpoint /lol-gameflow/v1/gameflow-phase
// "None"           → IDLE
// "Lobby"          → LOBBY
// "Matchmaking"    → MATCHMAKING
// "ReadyCheck"     → MATCH_FOUND
// "ChampSelect"    → BAN_PHASE / PICK_PHASE (checar timer)
// "GameStart"      → LOADING
// "InProgress"     → INGAME
// "EndOfGame"      → POSTGAME
// "WaitingForStats"→ POSTGAME
```

**Fluxo de detecção:**

```
LCU WebSocket → Rust Event Handler → GameStateManager → Tauri emit("game_state_changed") → Vue Store → UI reativa
```

**Checklist:**
- [ ] GameStateManager criado em Rust
- [ ] Todos os estados mapeados corretamente
- [ ] Transições de estado emitindo eventos Tauri
- [ ] Store Vue reativa a mudanças de estado
- [ ] UI principal adaptando layout por estado

---

### FASE 3 — Overlay System

**Objetivo:** Janelas transparentes, sempre no topo, que aparecem contextualmente.

---

#### Passo 5 — Overlay Base

Usando **Tauri multiwindow**: cada overlay é uma janela separada, configurada como:

```json
// tauri.conf.json
{
  "label": "overlay-runes",
  "transparent": true,
  "decorations": false,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "focus": false,
  "x": 0, "y": 0,
  "width": 1920, "height": 1080
}
```

- Click-through via `set_ignore_cursor_events(true)` por padrão
- Ativar interação apenas quando o jogador abrir o overlay intencionalmente (hotkey)
- Overlays individuais por contexto: runas, matchup, wards, alertas

**Checklist:**
- [ ] Janela overlay transparente criada
- [ ] Click-through funcionando
- [ ] Always-on-top funcionando
- [ ] Sistema de show/hide por estado de jogo
- [ ] Hotkey global para toggle

---

#### Passo 6 — Overlay de Runas e Build

Ativado quando: `PICK_PHASE` e campeão confirmado.

**Conteúdo:**
- Runas primárias e secundárias (visual idêntico ao cliente)
- Summoner spells recomendados
- Ordem de habilidades (Q/W/E/R por nível)
- Build de itens inicial (trinket, poções, itens de início)
- Fonte dos dados: SpellCoach API

**Layout sugerido:** painel lateral direito, ~300px de largura, entra com animação fade+slide.

---

#### Passo 7 — Overlay de Matchup

Ativado quando: campeão inimigo da lane detectado.

**Conteúdo:**
- Indicador visual: Favoreável / Neutro / Desfavorável
- Powerspikes do inimigo (níveis 2, 3, 6, 11, 16 quando relevante)
- Dificuldade da lane (1-10)
- Dica principal do matchup (ex: "Evite trades curtas antes do nível 6")
- Aviso de powerspike em tempo real durante a partida

---

### FASE 4 — Ban & Pick Assistant

**Objetivo:** Ajudar o jogador a tomar melhores decisões nas fases de ban e pick.

---

#### Passo 8 — Threat Engine

Análise de cada jogador inimigo detectado via LCU:

```typescript
interface ThreatAnalysis {
  summonerName: string;
  topChampions: ChampionStat[];   // os 3 campeões mais jogados recentemente
  recentWinrate: number;           // WR nos últimos 20 jogos
  masteryPeak: ChampionMastery;    // maior maestria
  isThreat: boolean;
  threatReason: string[];
}
```

**Fatores de ameaça:**
- Winrate > 60% nos últimos 20 jogos com o campeão
- Maestria alta (M6/M7) em campeão de alto snowball
- Matchup ruim para o campeão que você pretende jogar
- Campeão com histórico de abuso no patch atual

---

#### Passo 9 — Recomendação de Ban

UI no overlay durante `BAN_PHASE`:

```
🚫 Recomendação de Ban

┌─────────────────────────────────┐
│  [Draven]  Ban prioritário      │
│  ├─ 78% WR recente (14/20)      │
│  ├─ Matchup ruim para seu ADC   │
│  └─ Alto snowball potential     │
│                                 │
│  Alternativas: Caitlyn, Jinx    │
└─────────────────────────────────┘
```

---

### FASE 5 — OCR + Visão Computacional

**Objetivo:** Extrair informações da tela do jogo sem acesso à memória do processo.

---

#### Passo 10 — Sistema OCR

**Tecnologia:** Tesseract OCR via binding Rust (`leptess` crate) + screenshot via `scap` ou `xcap`.

**Regiões de captura e o que extrair:**

| Região da Tela | Informação Extraída | Frequência |
|---|---|---|
| Minimapa (canto inferior esquerdo) | Posições de inimigos | A cada 3s |
| KDA (placar superior) | Kills/Deaths/Assists do time | A cada 5s |
| Summoner spells (abaixo dos nomes) | Flash/Ignite/TP usados | A cada 2s |
| Timer de objetivos (área central superior) | Tempo de Dragão/Baron/Herald | A cada 5s |
| Chat (lado esquerdo) | MIA calls (ignorar se privacidade) | Quando aparecer |

**Pipeline OCR:**
```
Screenshot → Crop da região → Escala 2x → Filtro de contraste → Tesseract → Parser → Dado estruturado
```

**Pré-processamento essencial:**
- Converter para grayscale
- Aplicar threshold adaptativo
- Escalar imagem (OCR funciona melhor em imagens maiores)
- Remover ruído com filtro mediano

---

#### Passo 11 — MiniMap Vision AI

Análise do minimapa para detectar padrões de movimentação:

- **Inimigo desapareceu:** comparar posições conhecidas com as esperadas
- **Roam detectado:** midlaner sumiu + tempo passa → alerta "Mid missing"
- **Invade provável:** jungler inimigo não apareceu no mapa por X segundos
- **Objetivo crítico:** detectar agrupamento de inimigos perto de Dragão/Baron

> **Nota técnica:** Para a fase inicial, usar comparação de cores/regiões é mais rápido que ML. Reservar modelo de visão para versão futura.

---

### FASE 6 — Coach em Tempo Real (TTS)

**Objetivo:** Alertas de voz oportunos, concisos e acionáveis.

---

#### Passo 12 — Sistema de Voz TTS

```rust
// src-tauri/src/tts/queue.rs
pub struct TtsQueue {
    queue: VecDeque<TtsMessage>,
    is_playing: bool,
    cooldowns: HashMap<AlertCategory, Instant>,
}

pub struct TtsMessage {
    text: String,
    priority: u8,       // 1=info, 2=aviso, 3=crítico
    category: AlertCategory,
    cooldown_secs: u64, // evitar spam do mesmo alerta
}
```

**Regras da fila:**
- Mensagem crítica interrompe mensagem de info
- Cooldown por categoria (ex: "Top sem flash" não repete por 60s)
- Volume configurável por categoria
- Modo silencioso total com hotkey (ex: `F9`)

**Exemplos de alertas por timing:**

| Minuto | Alerta |
|---|---|
| 3:00 | "Primeiro dragão nasce em dois minutos" |
| 4:00 | "Scuttlecrab ativa" |
| 5:00 | "Dragão em um minuto" |
| 6:00 | "Dragão disponível agora" |
| 14:00 | "Primeiro herald em um minuto" |
| Qualquer | "Top sem flash" (quando detectado) |
| Qualquer | "Bot avançada sem visão de rio" |
| Qualquer | "Midlaner desapareceu, cuidado com roam" |

---

#### Passo 13 — Coach por Role

Cada role tem um conjunto de alertas específicos com condições próprias:

**🏹 ADC**
- "Inimigo com flash disponível, evite trades longas"
- "Suporte inimigo roubou um ward, vision comprometida"
- "Você está na range do inimigo sem vision de flancos"
- "Powerspike do inimigo no nível 6, jogue seguro"

**🛡️ Suporte**
- "Rio superior sem visão há 2 minutos"
- "Momento ideal para engajar: inimigo sem flash"
- "Você pode roamar agora: ADC tem wave boa"
- "Tempo ideal para ward ofensivo no tribush"

**🌲 Jungle**
- "Inimigo jungler apareceu top, bot está vulnerável"
- "Dragão em 90 segundos, priorize bot"
- "Você está na frente de farm em 15 minutos, considere invadir"
- "Herald disponível, midlaner tem prioridade para auxiliar"

**⚔️ Mid**
- "Você tem prioridade de wave, momento para roamar"
- "Roaming inimigo detectado, push rápido para compensar"
- "Skill principal disponível, oportunidade de trade"
- "Inimigo comprou Zhonya: cuidado com trades de all-in"

**🗡️ Top**
- "TP disponível, oportunidade de TP ofensivo"
- "Wave congelada: pressão o inimigo em direção à sua torre"
- "Inimigo sem TP, você pode jogar mais agressivo"
- "Teleporte inimigo CD: próximos 4 minutos são seus"

---

### FASE 7 — Smart Vision System (Wards)

**Objetivo:** Recomendar visão inteligente baseada no estado atual da partida.

---

#### Passo 14 — Sistema de Visão Estratégica

**Banco de pontos de visão** (pré-configurados por role/objetivo):

```typescript
interface WardSpot {
  id: string;
  name: string;
  x: number;            // coordenada no minimapa (0-1)
  y: number;
  type: 'offensive' | 'defensive' | 'objective' | 'pink';
  roles: Role[];        // quem deve colocar
  relevantWhen: WardCondition;
  priority: number;
}
```

**Overlay no minimapa:**
- Pontos pulsando nas localizações recomendadas
- Código de cores: 🟡 ward amarela, 🟣 ward de controle (pink), 🔵 informativa
- Temporizadores sobre wards já colocadas (quando detectadas via OCR)

**Condições de recomendação:**
- Dragão/Baron nascendo em < 2 min → mostrar wards de visão do objetivo
- Inimigo jungler na metade do mapa → wards defensivas
- Time com vantagem de ouro → wards ofensivas para pressionar

---

### FASE 8 — Pós-Game Review

**Objetivo:** Transformar cada partida em uma sessão de aprendizado.

---

#### Passo 15 — Match Analyzer

Dados coletados durante a partida e analisados ao final:

```typescript
interface GameTimeline {
  deaths: DeathEvent[];         // quando, onde, com quais condições
  idleTime: TimeRange[];        // tempo parado (> 15s sem CS ou movimento)
  visionGaps: TimeRange[];      // quando houve <2 wards ativas
  missedObjectives: Event[];    // objetivos que você poderia ter contestado
  wastedSpells: SpellEvent[];   // spells gastas em wave antes de trades
}
```

**Tela de revisão (PostGame.vue):**
- Timeline visual da partida com eventos marcados
- "Mortes evitáveis" destacadas (morreu sem vision, fora de posição)
- Comparação de CS/min com a média do rank
- Eficiência de visão vs média do rank
- Gráfico de ouro ao longo do tempo

---

#### Passo 16 — IA Coach (Feedback Textual)

Usar a SpellCoach API para gerar feedback qualitativo:

```typescript
const feedback = await coachApi.analyzeGame({
  champion: 'Jinx',
  role: 'ADC',
  timeline: gameTimeline,
  playerPatterns: currentPatterns,
});

// Exemplo de retorno:
// "Aos 8:20, você usou o Foguete (W) para limpar a wave antes de uma
//  trade importante. Isso custou 2 segundos de cooldown crítico.
//  Na próxima vez, considere guardar o W para a trade e limpar
//  a wave com auto-attacks."
```

---

### FASE 9 — Perfil Inteligente e Comportamental

**Objetivo:** Conhecer profundamente o estilo de jogo do usuário para personalizar o coaching.

---

#### Passo 17 — Player Profile System

Acumulando dados de múltiplas partidas:

```typescript
interface PlayerProfile {
  // Estilo
  playstyle: 'aggressive' | 'passive' | 'balanced';
  
  // Pontos fortes detectados
  strengths: string[];     // ["bom farmer", "bom em trades curtas"]
  
  // Padrões negativos
  weaknesses: string[];    // ["morre sem vision entre 12-15min", "ignora herald"]
  
  // Campeões com melhor performance
  bestChampions: ChampionStat[];
  
  // Evolução temporal
  improvementTrend: TrendData;  // últimas 4 semanas
}
```

---

#### Passo 18 — Coaching Personalizado

A partir do perfil, o sistema adapta os alertas:

- Se o padrão mostra mortes entre 12-15 min → intensificar alertas de vision nesse período
- Se winrate baixo com determinado matchup → priorizar aquele ban
- Se CS/min melhorou nas últimas 10 partidas → reconhecer o progresso em voz

**Tela de perfil (Profile.vue):**
- Radar chart: Farming / Visão / Macro / Trades / Objetivos
- Gráfico de evolução de WR por semana
- Lista de padrões recorrentes com dicas específicas
- "Conquistas" por milestones de melhoria

---

### FASE 10 — Performance e Otimização

**Objetivo:** App leve o suficiente para não afetar o FPS do jogo.

---

#### Passo 19 — Otimizações

**Metas de performance:**
- RAM: < 150MB durante partida (200MB pico)
- CPU: < 3% em idle, < 8% durante captura OCR
- Sem impacto mensurável no FPS do LoL

**Estratégias:**

| Área | Otimização |
|---|---|
| OCR | Throttle a 1 captura por região a cada 2-5s; processar em thread separada |
| Overlays | Renderizar apenas overlays visíveis; pausar Vue watchers dos ocultos |
| LCU polling | Usar WebSocket em vez de polling HTTP sempre que possível |
| Data Dragon | Cache agressivo local; não buscar na rede durante partida |
| SQLite | WAL mode, índices nas colunas mais consultadas, VACUUM mensal |
| TTS | Pré-cache de alertas comuns em arquivos de áudio locais |
| Imagens | Lazy loading de splash arts, thumbnails de 64px para listas |

---

### FASE 11 — Segurança e Compliance Riot

**Objetivo:** Manter o app em conformidade com os Termos de Serviço da Riot.

---

#### Passo 20 — Compliance

**Regras invioláveis:**

```
❌ NUNCA fazer:
  - Ler diretamente a memória do processo LeagueOfLegends.exe
  - Simular input de mouse ou teclado no jogo
  - Interceptar pacotes de rede do jogo
  - Injetar DLLs ou hooks no processo do LoL
  - Automatizar qualquer ação no cliente ou no jogo

✅ SEMPRE usar:
  - Apenas a LCU API oficial (documentada pela Riot)
  - Screenshot da tela para OCR (sem acesso ao processo)
  - Dados do Riot API público (com chave registrada)
  - Apenas informação passiva e assistiva
```

**Documentos de referência:**
- [Riot Games Third Party Policy](https://www.riotgames.com/en/legal)
- [LCU API Documentation (Unofficial)](https://lcu.vivide.re/)

**Registro no Riot Developer Portal:**
- Registrar a aplicação para obter chave de API oficial
- Usar endpoints do Riot Games API para dados históricos (não apenas LCU)
- Respeitar rate limits: 20 req/s, 100 req/2min para Development Key

---

## 8. Roadmap Resumido

```
MESES 1-2: Fundação
  ✓ Setup Tauri + Vue + SQLite
  ✓ LCU API conectada
  ✓ Game State Manager funcional
  ✓ Overlay base transparente

MESES 3-4: Features Principais
  ✓ Overlay de Runas e Matchup
  ✓ Ban Assistant com Threat Engine
  ✓ TTS com alertas básicos
  ✓ Coach por role (ADC/SUP primeiro)

MESES 5-6: Visão e Inteligência
  ✓ OCR funcional (sumoner spells, timer de objetivos)
  ✓ Smart Vision System
  ✓ Todos os roles no sistema de coach
  ✓ PostGame Review básico

MESES 7-8: Personalização e Polish
  ✓ Player Profile System
  ✓ Coaching personalizado
  ✓ PostGame Review com IA
  ✓ Otimizações de performance
  ✓ UI/UX refinada

MESES 9-10: Launch Prep
  ✓ MiniMap Vision AI
  ✓ Conquistas e gamificação
  ✓ Onboarding fluido
  ✓ Beta com usuários reais
  ✓ Ajustes de compliance
  ✓ Distribução via instalador (NSIS/MSI)
```

---

## 9. Decisões de Arquitetura e Boas Práticas

### Por que Tauri e não Electron?

| | Tauri | Electron |
|---|---|---|
| RAM | ~30-80MB | ~150-300MB |
| Tamanho do binário | ~5MB | ~80MB+ |
| Performance | Alta (Rust) | Moderada (Node.js) |
| Acesso nativo | Total | Limitado |
| Maturidade | Média | Alta |

Para um app que precisa ser **leve durante uma partida de jogo**, Tauri é a escolha correta.

### Gerenciamento de Estado

- **Backend (Rust):** estado autoritativo (LCU data, game phase, OCR results)
- **Frontend (Vue/Pinia):** estado derivado e reativo, sincronizado via Tauri events
- **Regra:** o frontend nunca inventa estado, apenas reflete o backend

### Tratamento de Erros

```rust
// Sempre retornar Result em commands Tauri
#[tauri::command]
async fn get_champion_data(id: u32) -> Result<Champion, String> {
    fetch_champion(id).await.map_err(|e| e.to_string())
}
```

```typescript
// Frontend sempre trata erros de commands
try {
  const champion = await invoke('get_champion_data', { id: 157 });
} catch (error) {
  console.error('Erro ao buscar campeão:', error);
  // fallback para cache local
}
```

### Versionamento e Updates

- Usar o sistema de updates automáticos do Tauri (`tauri-plugin-updater`)
- Servidor de updates: endpoint próprio ou GitHub Releases
- Updates silenciosos em background, aplicados ao reiniciar
- Versionar o schema do banco (migrations numeradas)

### Logs e Debug

- Usar `tracing` crate no Rust para logs estruturados
- Logs salvos em `AppData/RiftSync/logs/riftsync-YYYY-MM-DD.log`
- Nível de log configurável (DEBUG só em modo desenvolvimento)
- Painel de debug na UI (acessível com `Ctrl+Shift+D`) mostrando estado atual

---

> **RiftSync AI** — *Do bronze ao challenger, um alerta de cada vez.*
