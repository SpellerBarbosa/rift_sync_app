// ============================================================
// lcu/ — Cliente da League Client Update (LCU) API
//
// Sub-módulos:
//   lockfile   — lê e parseia o lockfile do LoL (porta + token)
//   client     — HTTP client com bypass SSL para a LCU API
//   websocket  — WebSocket para eventos em tempo real
// ============================================================

pub mod client;
pub mod lockfile;
pub mod websocket;
