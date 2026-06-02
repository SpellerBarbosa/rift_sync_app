// Suprime janela de console no Windows em builds de produção
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Delega para a biblioteca (padrão Tauri 2 — compartilhado com mobile)
    riftsync_ai_lib::run()
}
