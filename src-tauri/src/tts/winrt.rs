// ============================================================
// tts/winrt.rs — TTS local via Windows.Media.SpeechSynthesis (WinRT)
//
// Usa PowerShell para acessar a API WinRT SpeechSynthesizer, que
// expõe as vozes NEURAIS do Windows 10/11 (Aria, Davis, Francisca…).
// O PowerShell 5.1 consegue acessar WinRT via ContentType=WindowsRuntime
// e converter IAsyncOperation para Task via System.Runtime.WindowsRuntime.
//
// Fallback automático para System.Speech.Synthesis (SAPI) caso WinRT
// não esteja disponível (muito improvável no Win10+).
// ============================================================

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use std::process::Command;

// ── Ponto de entrada público ──────────────────────────────────

/// Sintetiza texto com voz neural (WinRT) ou SAPI (fallback).
/// Retorna `data:audio/wav;base64,...` pronto para `new Audio(url)`.
/// Chamar apenas de `tokio::task::spawn_blocking`.
#[cfg(windows)]
pub fn synthesize_blocking(text: String, voice_name: String) -> Result<String> {
    let temp_path = {
        let dir = std::env::temp_dir();
        dir.join(format!("rs_tts_{}.wav", uuid::Uuid::new_v4()))
    };
    let path_str   = temp_path.display().to_string();
    let text_safe  = text.replace('\'', "''");
    let path_safe  = path_str.replace('\'', "''");
    let voice_safe = voice_name.replace('\'', "''");

    // Script: tenta WinRT neural primeiro, cai para SAPI se falhar
    let script = format!(r#"
$ErrorActionPreference = 'Stop'
$text  = '{text}'
$voice = '{voice}'
$out   = '{path}'

function Await($op) {{
    $task = [System.WindowsRuntimeSystemExtensions]::AsTask($op)
    $task.GetAwaiter().GetResult()
}}

try {{
    # ── WinRT (vozes neurais) ─────────────────────────────────
    [void][Windows.Media.SpeechSynthesis.SpeechSynthesizer, Windows.Media.SpeechSynthesis, ContentType=WindowsRuntime]
    [void][Windows.Storage.Streams.DataReader, Windows.Storage.Streams, ContentType=WindowsRuntime]

    $rtDir = [System.Runtime.InteropServices.RuntimeEnvironment]::GetRuntimeDirectory()
    Add-Type -Path "$rtDir\System.Runtime.WindowsRuntime.dll" -ErrorAction SilentlyContinue

    $synth = [Windows.Media.SpeechSynthesis.SpeechSynthesizer]::new()

    if ($voice -ne '') {{
        $match = [Windows.Media.SpeechSynthesis.SpeechSynthesizer]::AllVoices |
                 Where-Object {{ $_.DisplayName -eq $voice }} |
                 Select-Object -First 1
        if ($match) {{ $synth.Voice = $match }}
    }}

    $stream = Await ($synth.SynthesizeTextToStreamAsync($text))
    $size   = [uint32]$stream.Size
    $reader = [Windows.Storage.Streams.DataReader]::CreateDataReader($stream.GetInputStreamAt(0))
    Await ($reader.LoadAsync($size)) | Out-Null
    $bytes  = New-Object byte[] $size
    $reader.ReadBytes($bytes)
    [System.IO.File]::WriteAllBytes($out, $bytes)

}} catch {{
    # ── SAPI fallback ─────────────────────────────────────────
    Add-Type -AssemblyName System.Speech
    $s = New-Object System.Speech.Synthesis.SpeechSynthesizer
    if ($voice -ne '') {{ try {{ $s.SelectVoice($voice) }} catch {{}} }}
    $s.SetOutputToWaveFile($out)
    $s.Speak($text)
    $s.Dispose()
}}
"#,
        text  = text_safe,
        voice = voice_safe,
        path  = path_safe,
    );

    tracing::debug!("[TTS] sintetizando voz='{}' texto='{:.50}'", voice_name, text);

    let out = Command::new("powershell.exe")
        .args(["-NonInteractive", "-NoProfile", "-Command", &script])
        .output()
        .context("Falha ao executar PowerShell")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("PowerShell TTS falhou ({}): {}", out.status, stderr.trim());
    }

    if !temp_path.exists() {
        anyhow::bail!("Arquivo WAV não foi gerado. stderr: {}", String::from_utf8_lossy(&out.stderr).trim());
    }

    let wav_bytes = std::fs::read(&temp_path).context("Falha ao ler WAV")?;
    let _         = std::fs::remove_file(&temp_path);

    tracing::debug!("[TTS] {} bytes gerados para '{:.50}'", wav_bytes.len(), text);

    Ok(format!("data:audio/wav;base64,{}", B64.encode(&wav_bytes)))
}

/// Lista vozes instaladas — usa WinRT AllVoices (inclui neurais) com
/// fallback para SAPI GetInstalledVoices.
/// Cada tupla: (id_único, display_name, locale)
#[cfg(windows)]
pub fn list_system_voices() -> Result<Vec<(String, String, String)>> {
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'

try {
    [void][Windows.Media.SpeechSynthesis.SpeechSynthesizer, Windows.Media.SpeechSynthesis, ContentType=WindowsRuntime]
    $voices = [Windows.Media.SpeechSynthesis.SpeechSynthesizer]::AllVoices
    foreach ($v in $voices) {
        # VoiceGender: Male=0, Female=1
        $g = if ([int]$v.Gender -eq 0) { 'Male' } else { 'Female' }
        Write-Output ($v.DisplayName + '|' + $v.Language + '|' + $g)
    }
} catch {
    Add-Type -AssemblyName System.Speech
    $s = New-Object System.Speech.Synthesis.SpeechSynthesizer
    $s.GetInstalledVoices() | ForEach-Object {
        $v = $_.VoiceInfo
        Write-Output ($v.Name + '|' + $v.Culture.Name + '|' + $v.Gender)
    }
    $s.Dispose()
}
"#;

    let out = Command::new("powershell.exe")
        .args(["-NonInteractive", "-NoProfile", "-Command", script])
        .output()
        .context("Falha ao listar vozes")?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut voices = Vec::new();
    let mut seen   = std::collections::HashSet::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() < 3 { continue; }

        let name   = parts[0].trim().to_string();
        let locale = parts[1].trim().to_string();
        let gender = parts[2].trim().to_lowercase();

        if name.is_empty() || seen.contains(&name) { continue; }
        seen.insert(name.clone());

        let g_char = if gender.contains("female") || gender == "1" { "f" } else { "m" };
        let id     = format!("{}_{}_{}", locale.to_lowercase(), g_char, voices.len());
        voices.push((id, name, locale));
    }

    tracing::debug!("[TTS] {} vozes encontradas no sistema", voices.len());
    Ok(voices)
}

// ── Stubs para não-Windows ────────────────────────────────────

#[cfg(not(windows))]
pub fn synthesize_blocking(_text: String, _voice_id: String) -> Result<String> {
    anyhow::bail!("Windows TTS disponível apenas no Windows")
}

#[cfg(not(windows))]
pub fn list_system_voices() -> Result<Vec<(String, String, String)>> {
    Ok(Vec::new())
}
