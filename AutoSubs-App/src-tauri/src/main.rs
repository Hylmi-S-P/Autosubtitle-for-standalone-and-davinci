// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tauri::{Manager, RunEvent};
use tauri::Emitter; // for app.emit
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use tokio::sync::Notify;

use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

// Import plugins
use tauri_plugin_fs::init as fs_plugin;
use tauri_plugin_http::init as http_plugin;
use tauri_plugin_process::init as process_plugin;
use tauri_plugin_shell::init as shell_plugin;
use tauri_plugin_shell::ShellExt; // for app.shell()
use tauri_plugin_store::Builder as StoreBuilder;
use tauri_plugin_clipboard_manager::init as clipboard_plugin;
use tauri_plugin_opener::init as opener_plugin;
use tokio::process::Command as TokioCommand;

mod audio_preprocess;
mod models;
mod transcription_api;
mod transcript_types;
mod logging;

// Include integration-like tests that need crate visibility
#[cfg(test)]
mod tests;

// Global guard to avoid re-entrant exit handling
static EXITING: AtomicBool = AtomicBool::new(false);



use std::sync::{Mutex, OnceLock};

static LAST_DOCKER_TRANSCRIPT: OnceLock<Mutex<Option<(String, serde_json::Value)>>> = OnceLock::new();

/// Validates that a path is within the allowed directories (Desktop, Documents, Downloads, or Resource).
pub fn is_path_allowed<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>, path: &std::path::Path) -> bool {
    let canonical_path = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => return false,
    };

    let allowed_dirs = [
        app_handle.path().desktop_dir(),
        app_handle.path().document_dir(),
        app_handle.path().download_dir(),
        app_handle.path().resource_dir(),
    ];

    for dir in allowed_dirs {
        if let Ok(allowed_path) = dir {
            if let Ok(canonical_allowed) = allowed_path.canonicalize() {
                if canonical_path.starts_with(canonical_allowed) {
                    return true;
                }
            }
        }
    }

    false
}

/// Send a local audio file to a Dockerized Faster-Whisper server and return
/// the transcription as raw SRT text.
///
/// This is an alternative transcription route that bypasses the built-in
/// transcription engine. The existing engine logic is left completely intact.
#[tauri::command]
async fn transcribe_with_docker(
    app_handle: tauri::AppHandle,
    file_path: String,
    translate: Option<bool>,
    target_language: Option<String>,
) -> Result<String, String> {
    // Security check: ensure the file_path is within allowed directories
    if !is_path_allowed(&app_handle, std::path::Path::new(&file_path)) {
        return Err("Access to the specified file is denied: outside of allowed scope".to_string());
    }

    // 1. Check if we already have the transcript for this file cached
    let cache_mutex = LAST_DOCKER_TRANSCRIPT.get_or_init(|| Mutex::new(None));
    let cached_json = {
        let guard = cache_mutex.lock().unwrap();
        if let Some(data) = guard.as_ref() {
            if data.0 == file_path {
                Some(data.1.clone())
            } else {
                None
            }
        } else {
            None
        }
    };

    let json_response: serde_json::Value = if let Some(json) = cached_json {
        println!("DEBUG: Using cached transcription for {}", file_path);
        json
    } else {
        // 2. Read the audio file from disk
        let file_bytes = tokio::fs::read(&file_path)
            .await
            .map_err(|e| format!("Failed to read audio file '{}': {}", file_path, e))?;

        let file_name = std::path::Path::new(&file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio.wav")
            .to_string();

        // 3. Build multipart form
        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("application/octet-stream")
            .map_err(|e| format!("Failed to build file part: {}", e))?;

        let form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", "deepdml/faster-whisper-large-v3-turbo-ct2")
            .text("response_format", "verbose_json");

        // 4. Send the request to the local Docker API
        let client = reqwest::Client::new();
        let response = client
            .post("http://localhost:8000/v1/audio/transcriptions")
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("HTTP request to Docker Faster-Whisper API failed: {}", e))?;

        // 5. Check for a successful status code
        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<could not read response body>".to_string());
            return Err(format!(
                "Docker API returned HTTP {}: {}",
                status, body
            ));
        }

        // 6. Parse the verbose_json response
        let json_response: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse JSON response: {}", e))?;
            
        // Cache it for future reuse
        if let Ok(mut guard) = cache_mutex.lock() {
            *guard = Some((file_path.clone(), json_response.clone()));
        }
        
        json_response
    };

    // 6. Convert to transcription_engine::Segment objects
    let mut segments = Vec::new();
    if let Some(segs_arr) = json_response.get("segments").and_then(|v| v.as_array()) {
        for s in segs_arr {
            let start = s.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let end = s.get("end").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let text = s.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            
            let mut words = Vec::new();
            if let Some(words_arr) = s.get("words").and_then(|v| v.as_array()) {
                for w in words_arr {
                    words.push(transcription_engine::WordTimestamp {
                        text: w.get("word").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        start: w.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        end: w.get("end").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        probability: w.get("probability").and_then(|v| v.as_f64()).map(|v| v as f32),
                    });
                }
            }
            
            segments.push(transcription_engine::Segment {
                start,
                end,
                text,
                words: if words.is_empty() { None } else { Some(words) },
                speaker_id: None,
            });
        }
    }

    // 7. Perform translation if requested
    if translate.unwrap_or(false) {
        if let Some(target) = target_language {
            // Do not translate if target is auto
            if target != "auto" {
                let detected_lang = json_response.get("language").and_then(|v| v.as_str()).unwrap_or("auto");
                println!("DEBUG: Docker route initiating translation from {} to {}", detected_lang, target);
                // Call translate_segments
                if let Err(e) = transcription_engine::translate_segments(&mut segments, detected_lang, &target, None).await {
                    println!("ERROR: Docker route translation failed: {}", e);
                }
            }
        }
    }

    // 8. Convert to SRT format for the frontend
    let mut srt_out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        // Format times to HH:MM:SS,MMM
        let format_time = |time: f64| -> String {
            let hours = (time / 3600.0) as u32;
            let mins = ((time % 3600.0) / 60.0) as u32;
            let secs = (time % 60.0) as u32;
            let millis = ((time.fract()) * 1000.0) as u32;
            format!("{:02}:{:02}:{:02},{:03}", hours, mins, secs, millis)
        };
        
        srt_out.push_str(&format!("{}\n{} --> {}\n{}\n\n", 
            i + 1, 
            format_time(seg.start), 
            format_time(seg.end), 
            seg.text.trim()
        ));
    }

    Ok(srt_out)
}

#[tauri::command]
async fn delete_transcript(filename: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use std::fs;
    use std::path::{Component, Path};

    // Prevent path traversal by ensuring the filename is a simple filename and not a path
    let mut components = Path::new(&filename).components();
    let is_safe = match components.next() {
        Some(Component::Normal(n)) => {
            components.next().is_none() && n.to_str().map(|s| !s.contains('/') && !s.contains('\\')).unwrap_or(false)
        }
        _ => false,
    };

    if !is_safe {
        return Err("Invalid filename: path traversal or absolute paths not allowed".to_string());
    }

    let path = app_handle.path().document_dir()
        .map_err(|e| e.to_string())?
        .join("AutoSubs-Transcripts")
        .join(&filename);

    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to delete transcript file: {}", e))?;
    }

    // Also update the index file if needed, but it's easier to let the frontend handle the index update via its logic
    // or just delete the index item when the frontend re-lists files.
    
    Ok(())
}

#[tauri::command]
async fn delete_all_transcripts(app_handle: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use std::fs;

    let dir_path = app_handle.path().document_dir()
        .map_err(|e| e.to_string())?
        .join("AutoSubs-Transcripts");

    if dir_path.exists() {
        // We delete everything inside the directory
        for entry in fs::read_dir(&dir_path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_file() {
                fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

fn main() {
    // Note: whisper-diarize-rs handles whisper_rs logging internally
    tauri::Builder::default()

        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(StoreBuilder::default().build())
        // Register each plugin
        .plugin(http_plugin())
        .plugin(fs_plugin())
        .plugin(process_plugin())
        .plugin(shell_plugin())
        .plugin(clipboard_plugin())
        .plugin(opener_plugin())
        .setup(|app| {
            // Initialize backend logging (file + in-memory ring buffer)
            crate::logging::init_logging(&app.handle());

            // Startup sidecar health check: ffmpeg availability & version
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut ffmpeg_ok = false;
                    let mut ffmpeg_version = String::new();

                    // ffmpeg -version (sidecar first, then system fallback)
                    match app_handle.shell().sidecar("ffmpeg") {
                        Ok(cmd) => {
                            match cmd.args(["-version"]).output().await {
                                Ok(out) if out.status.success() => {
                                    ffmpeg_ok = true;
                                    let stdout = String::from_utf8_lossy(&out.stdout);
                                    ffmpeg_version = stdout.lines().next().unwrap_or("").to_string();
                                    tracing::info!("ffmpeg check (sidecar): ok=true, version=\"{}\"", ffmpeg_version);
                                }
                                Ok(out) => {
                                    let stderr = String::from_utf8_lossy(&out.stderr);
                                    tracing::warn!("ffmpeg sidecar -version exited non-zero. stderr: {}", stderr);
                                    // fallback to system
                                    if let Ok(sys) = TokioCommand::new("ffmpeg").arg("-version").output().await {
                                        ffmpeg_ok = sys.status.success();
                                        let stdout = String::from_utf8_lossy(&sys.stdout);
                                        ffmpeg_version = stdout.lines().next().unwrap_or("").to_string();
                                        tracing::info!("ffmpeg check (system): ok={}, version=\"{}\"", ffmpeg_ok, ffmpeg_version);
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("ffmpeg sidecar execution failed: {:?}", e);
                                    if let Ok(sys) = TokioCommand::new("ffmpeg").arg("-version").output().await {
                                        ffmpeg_ok = sys.status.success();
                                        let stdout = String::from_utf8_lossy(&sys.stdout);
                                        ffmpeg_version = stdout.lines().next().unwrap_or("").to_string();
                                        tracing::info!("ffmpeg check (system): ok={}, version=\"{}\"", ffmpeg_ok, ffmpeg_version);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("ffmpeg sidecar not found/failed to init: {:?}", e);
                            if let Ok(sys) = TokioCommand::new("ffmpeg").arg("-version").output().await {
                                ffmpeg_ok = sys.status.success();
                                let stdout = String::from_utf8_lossy(&sys.stdout);
                                ffmpeg_version = stdout.lines().next().unwrap_or("").to_string();
                                tracing::info!("ffmpeg check (system): ok={}, version=\"{}\"", ffmpeg_ok, ffmpeg_version);
                            }
                        }
                    }

                    // Emit an event to frontend so users can access diagnostics quickly
                    let payload = json!({
                        "ffmpeg_ok": ffmpeg_ok,
                        "ffmpeg_version": ffmpeg_version,
                    });
                    let _ = app_handle.emit("sidecar-health", payload);

                    if !ffmpeg_ok {
                        tracing::warn!("One or more sidecars appear unavailable. On Windows, AV or SmartScreen may block sidecars; try 'Open Logs Folder' for details.");
                    }
                });
            }



            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            transcription_api::transcribe_audio,
            transcription_api::cancel_transcription,
            transcription_api::reformat_subtitles,
            models::get_downloaded_models,
            models::delete_model,
            logging::get_backend_logs,
            logging::clear_backend_logs,
            logging::get_log_dir,
            logging::export_backend_logs,

            transcribe_with_docker,
            delete_transcript,
            delete_all_transcripts
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri application")
        .run(|app, event| {
            match event {
                RunEvent::ExitRequested { api, .. } => {
                    // If we're already exiting, don't intercept again; allow exit to proceed
                    if EXITING.swap(true, AtomicOrdering::SeqCst) {
                        return;
                    }

                    // keep the app alive long enough to send the shutdown signal
                    api.prevent_exit();

                    // Proactively cancel any active long-running tasks (e.g., transcription)
                    if let Ok(mut should_cancel) = crate::transcription_api::SHOULD_CANCEL.lock() {
                        *should_cancel = true;
                    }

                    // Windows: do a small blocking send inline so we don't exit before the request is on the wire
                    #[cfg(target_os = "windows")]
                    {
                        let url = "http://127.0.0.1:56002/";
                        let bc = reqwest::blocking::Client::builder()
                            .no_proxy()
                            .tcp_nodelay(true)
                            .timeout(Duration::from_millis(800))
                            .build();
                        if let Ok(bc) = bc {
                            let _ = bc
                                .post(url)
                                .header("Connection", "close")
                                .json(&json!({ "func": "Exit" }))
                                .send();
                        }

                        // As an extra-safe fallback, send a raw HTTP request over TCP synchronously
                        {
                            use std::io::Write;
                            use std::net::TcpStream;
                            let body = b"{\"func\":\"Exit\"}";
                            let req = format!(
                                "POST / HTTP/1.1\r\nHost: 127.0.0.1:56002\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                                body.len()
                            );
                            if let Ok(mut stream) = TcpStream::connect_timeout(
                                &"127.0.0.1:56002".parse().unwrap(),
                                Duration::from_millis(400),
                            ) {
                                let _ = stream.set_nodelay(true);
                                let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
                                let _ = stream.write_all(req.as_bytes());
                                let _ = stream.write_all(body);
                                let _ = stream.flush();
                            }
                        }
                        // brief pause to allow flush
                        std::thread::sleep(Duration::from_millis(250));

                        // now actually exit the app
                        app.exit(0);

                        // last resort hard exit after a grace period
                        std::thread::spawn(|| {
                            std::thread::sleep(Duration::from_millis(1200));
                            std::process::exit(0);
                        });
                    }

                    // Non-Windows: keep async path
                    #[cfg(not(target_os = "windows"))]
                    {
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            // short timeout to avoid hanging on exit
                            let client = Client::builder()
                                .no_proxy()
                                .tcp_nodelay(true)
                                .timeout(Duration::from_millis(750))
                                .build()
                                .unwrap_or_else(|_| Client::new());

                            let url = "http://127.0.0.1:56002/";
                            let _ = client
                                .post(url)
                                .header("Connection", "close")
                                .json(&json!({ "func": "Exit" }))
                                .send()
                                .await;

                            tokio::time::sleep(Duration::from_millis(150)).await;
                            app_handle.exit(0);
                        });
                    }
                }
                RunEvent::WindowEvent { event, .. } => {
                    // Ensure clicking the window close (X) reliably routes through ExitRequested
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        if !EXITING.load(AtomicOrdering::SeqCst) {
                            app.exit(0);
                        }
                    }
                }
                _ => {}
            }
        });
}
