// ============================================================
// commands/ — Tauri commands expostos ao frontend Vue
//
// Cada módulo agrupa commands por domínio.
// Todos os commands devem retornar Result<T, String> para que
// o frontend possa capturar erros via try/catch no invoke().
// ============================================================

pub mod builds;
pub mod champion;
pub mod coach;
pub mod game_state;
pub mod loading;
pub mod player;
pub mod window;
