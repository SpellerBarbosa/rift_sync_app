fn main() {
    // Lê o .env em compile time e injeta as variáveis no binário via cargo:rustc-env.
    // Isso garante que as chaves funcionem mesmo sem o arquivo .env em runtime
    // (necessário para builds de produção instaladas pelo usuário).
    if let Ok(contents) = std::fs::read_to_string(".env") {
        for line in contents.lines() {
            // Remove BOM UTF-8 que o PowerShell pode inserir na primeira linha
            let line = line.trim_start_matches('\u{feff}').trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                println!("cargo:rustc-env={}={}", key.trim(), value.trim());
            }
        }
    }
    // Recompila se o .env mudar
    println!("cargo:rerun-if-changed=.env");

    tauri_build::build()
}
