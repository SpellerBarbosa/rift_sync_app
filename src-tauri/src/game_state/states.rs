// ============================================================
// game_state/states.rs — Enum GamePhase com mapeamentos LCU
//
// Mapeia as strings brutas da LCU API para um enum tipado
// que é usado pelo frontend (via eventos Tauri) e pelo backend.
//
// Ref: /lol-gameflow/v1/gameflow-phase
// ============================================================

use serde::{Deserialize, Serialize};

/// Fase atual do ciclo de vida de uma sessão de League of Legends.
/// Cada variante corresponde a uma tela específica no cliente LoL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GamePhase {
    /// LoL fechado ou na tela inicial — nenhuma atividade detectada
    Idle,
    /// Jogador criou ou entrou em um lobby
    Lobby,
    /// Na fila (aguardando partida)
    Matchmaking,
    /// Tela de aceitar partida (pop-up "Partida Encontrada")
    MatchFound,
    /// Fase de banimento de campeões
    BanPhase,
    /// Fase de seleção de campeões
    PickPhase,
    /// Tela de carregamento (entre o fim da seleção e o início do jogo)
    Loading,
    /// Partida em andamento
    InGame,
    /// Tela de resultados / pós-game
    PostGame,
}

impl GamePhase {
    /// Converte a string bruta da LCU API para o enum.
    /// Retorna `None` para fases desconhecidas (sem crashar).
    pub fn from_lcu_str(s: &str) -> Option<Self> {
        match s {
            "None"                => Some(Self::Idle),
            "Lobby"               => Some(Self::Lobby),
            "Matchmaking"         => Some(Self::Matchmaking),
            "ReadyCheck"          => Some(Self::MatchFound),
            "ChampSelect"         => Some(Self::PickPhase),   // distinguido via timer se necessário
            "GameStart"           => Some(Self::Loading),
            "InProgress"          => Some(Self::InGame),
            "EndOfGame"
            | "WaitingForStats"
            | "PreEndOfGame"      => Some(Self::PostGame),
            "Reconnect"           => Some(Self::InGame),
            "WatchInProgress"     => Some(Self::InGame),  // replay support
            _ => {
                tracing::debug!("GamePhase desconhecida recebida da LCU: '{s}'");
                None
            }
        }
    }

    /// Serializa para a string esperada pelo frontend (store gameState.ts).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle        => "IDLE",
            Self::Lobby       => "LOBBY",
            Self::Matchmaking => "MATCHMAKING",
            Self::MatchFound  => "MATCH_FOUND",
            Self::BanPhase    => "BAN_PHASE",
            Self::PickPhase   => "PICK_PHASE",
            Self::Loading     => "LOADING",
            Self::InGame      => "INGAME",
            Self::PostGame    => "POSTGAME",
        }
    }

    /// Retorna `true` se estamos dentro de uma partida ativa.
    pub fn is_in_game(&self) -> bool {
        matches!(self, Self::Loading | Self::InGame)
    }

    /// Retorna `true` se estamos em fase de seleção de campeões.
    pub fn is_in_champ_select(&self) -> bool {
        matches!(self, Self::BanPhase | Self::PickPhase)
    }
}

impl Default for GamePhase {
    fn default() -> Self {
        Self::Idle
    }
}

// ── Testes ───────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapeamento_lcu_strings() {
        assert_eq!(GamePhase::from_lcu_str("None"),       Some(GamePhase::Idle));
        assert_eq!(GamePhase::from_lcu_str("ChampSelect"),Some(GamePhase::PickPhase));
        assert_eq!(GamePhase::from_lcu_str("InProgress"), Some(GamePhase::InGame));
        assert_eq!(GamePhase::from_lcu_str("EndOfGame"),  Some(GamePhase::PostGame));
        assert_eq!(GamePhase::from_lcu_str("Desconhecida"), None);
    }

    #[test]
    fn serializa_para_frontend() {
        assert_eq!(GamePhase::InGame.as_str(),   "INGAME");
        assert_eq!(GamePhase::BanPhase.as_str(), "BAN_PHASE");
    }
}
