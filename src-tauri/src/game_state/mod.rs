// ============================================================
// game_state/ — Máquina de estados da partida
//
// Sub-módulos:
//   states   — enum GamePhase e conversões de/para strings LCU
//   manager  — loop de polling que detecta transições de estado
// ============================================================

pub mod manager;
pub mod states;
