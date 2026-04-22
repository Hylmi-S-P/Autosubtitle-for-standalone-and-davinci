use crate::transcription_api::{transcribe_audio, FrontendTranscribeOptions};
use tauri::test::{mock_builder, mock_context, noop_assets};
use std::fs;

#[cfg(test)]
mod tests {
    use super::*;

    // run with cargo test transcribe_audio_smoke -- --nocapture
    #[tokio::test(flavor = "multi_thread")]
    async fn transcribe_audio_smoke() {
        let app = mock_builder()
            .plugin(tauri_plugin_shell::init())
            .build(mock_context(noop_assets()))
            .expect("failed to build test app");
        let handle = app.handle().clone();

        // Use a portable test asset path (avoid absolute machine paths)
        let wav = format!("{}/tests/data/test-audio.wav", env!("CARGO_MANIFEST_DIR"));

        let options = FrontendTranscribeOptions {
            audio_path: wav,
            offset: None,
            model: "tiny.en".into(),
            lang: Some("en".into()),
            translate: Some(false),
            target_language: None,
            enable_dtw: Some(false),
            enable_gpu: Some(true),
            enable_diarize: Some(false),
            max_speakers: None,
            density: None,
            max_lines: None,

        };

        let res = transcribe_audio(handle, options).await;
        assert!(res.is_ok(), "transcription failed: {:?}", res.err());

        // Save resulting transcript to tests/data for inspection
        if let Ok(transcript) = res {
            let out_path = format!(
                "{}/tests/data/transcript-smoke.json",
                env!("CARGO_MANIFEST_DIR")
            );
            let json = serde_json::to_string_pretty(&transcript)
                .expect("failed to serialize transcript");
            fs::write(&out_path, json).expect("failed to write transcript file");
            eprintln!("Saved transcript to {}", out_path);
        }
    }

    // Runs transcription while ensuring VAD model is present; saves a VAD transcript snapshot.
    // run with: cargo test transcribe_audio_with_vad -- --nocapture
    #[tokio::test(flavor = "multi_thread")]
    async fn transcribe_audio_with_vad() {
        let app = mock_builder()
            .plugin(tauri_plugin_shell::init())
            .build(mock_context(noop_assets()))
            .expect("failed to build test app");
        let handle = app.handle().clone();

        let wav = format!("{}/tests/data/jfk.wav", env!("CARGO_MANIFEST_DIR"));

        let options = FrontendTranscribeOptions {
            audio_path: wav,
            offset: None,
            model: "tiny.en".into(),
            lang: Some("en".into()),
            translate: Some(false),
            target_language: None,
            enable_dtw: Some(true),
            enable_gpu: Some(true),
            enable_diarize: Some(true),
            max_speakers: None,
            density: None,
            max_lines: None,

        };

        let res = transcribe_audio(handle, options).await;
        assert!(res.is_ok(), "VAD transcription failed: {:?}", res.err());

        if let Ok(transcript) = res {
            let out_path = format!(
                "{}/tests/data/transcript-vad.json",
                env!("CARGO_MANIFEST_DIR")
            );
            let json = serde_json::to_string_pretty(&transcript)
                .expect("failed to serialize VAD transcript");
            fs::write(&out_path, json).expect("failed to write VAD transcript file");
            eprintln!("Saved VAD transcript to {}", out_path);
        }
    }

    #[tokio::test]
    async fn test_path_traversal_vulnerability_fixed() {
        use crate::delete_transcript;
        use tauri::Manager;

        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("failed to build test app");
        let handle = app.handle().clone();

        let doc_dir = handle.path().document_dir().expect("failed to get doc dir");
        let transcript_dir = doc_dir.join("AutoSubs-Transcripts");
        fs::create_dir_all(&transcript_dir).expect("failed to create transcript dir");

        // Create a "secret" file outside the transcripts directory
        let secret_file_path = doc_dir.join("vulnerable_secret_fixed.txt");
        fs::write(&secret_file_path, "this is a secret").expect("failed to write secret file");
        assert!(secret_file_path.exists());

        // Attempt to delete it via path traversal
        let traversal_filename = "../vulnerable_secret_fixed.txt".to_string();
        let result = delete_transcript(traversal_filename, handle.clone()).await;

        // It should return an error now
        assert!(
            result.is_err(),
            "delete_transcript should return Err when path traversal is attempted"
        );
        assert_eq!(
            result.unwrap_err(),
            "Invalid filename: path traversal or absolute paths not allowed"
        );

        // Check if the secret file still exists
        assert!(
            secret_file_path.exists(),
            "Secret file should NOT have been deleted!"
        );

        // Clean up
        fs::remove_file(&secret_file_path).ok();
    }

    #[tokio::test]
    async fn test_delete_transcript_normal() {
        use crate::delete_transcript;
        use tauri::Manager;

        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("failed to build test app");
        let handle = app.handle().clone();

        let doc_dir = handle.path().document_dir().expect("failed to get doc dir");
        let transcript_dir = doc_dir.join("AutoSubs-Transcripts");
        fs::create_dir_all(&transcript_dir).expect("failed to create transcript dir");

        let filename = "normal_transcript.txt".to_string();
        let file_path = transcript_dir.join(&filename);
        fs::write(&file_path, "some content").expect("failed to write transcript file");
        assert!(file_path.exists());

        let result = delete_transcript(filename, handle).await;
        assert!(result.is_ok());
        assert!(!file_path.exists());
    }
}