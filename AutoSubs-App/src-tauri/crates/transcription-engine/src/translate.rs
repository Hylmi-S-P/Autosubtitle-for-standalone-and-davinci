use reqwest;
use serde::{Serialize, Deserialize};
use serde_json::json;
use crate::types::{Segment, WordTimestamp, LabeledProgressFn, ProgressType};
use futures::stream::{self, StreamExt};
use tokio::time::{sleep, Duration};

// Normalize Whisper language codes to the codes accepted by the unofficial Google
// Translate endpoint. Applies both to source (sl) and target (tl) codes.
fn normalize_google_lang(code: &str, is_target: bool) -> String {
    let c = code.trim();
    if c.eq_ignore_ascii_case("auto") {
        return "auto".to_string();
    }

    // Canonicalize casing and hyphens
    let c = c.to_string();

    // Special cases first
    match c.as_str() {
        // Whisper uses "jw" for Javanese; Google expects "jv"
        "jw" => return "jv".to_string(),
        // Cantonese not supported separately: map to Traditional Chinese
        "yue" => return "zh-TW".to_string(),
        // Hebrew "he" is accepted; older "iw" also exists, so keep "he"
        _ => {}
    }

    // Target-specific adjustments
    if is_target {
        // Nynorsk often unsupported; map to general Norwegian
        if c == "nn" { return "no".to_string(); }
        if c == "yue" { return "zh-TW".to_string(); }
        if c == "jw" { return "jv".to_string(); }
    }

    c
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: String,
}

/// Translates text from one language to another using a local OpenAI-compatible service.
pub async fn translate_text(text: &str, from: &str, to: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Ollama can take several seconds to load a model (e.g., 14s in your logs).
    // We set a long timeout and more retries to accommodate this.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    
    // Switch to 127.0.0.1 to ensure we hit the IPv4 address Ollama is listening on
    let url = "http://127.0.0.1:11434/v1/chat/completions";
    
    // Using the confirmed model name from your Ollama list
    let model = "translategemma:12b"; 
    
    println!("DEBUG: Starting translation pass for segment: '{}'", text.chars().take(20).collect::<String>());

    let sl = normalize_google_lang(from, false);
    let tl = normalize_google_lang(to, true);

    let (full_sl, prompt_sl) = get_language_info(&sl);
    let (full_tl, prompt_tl) = get_language_info(&tl);

    let prompt = if sl == "auto" {
        format!(
            "You are a professional translator to {} ({}). Your goal is to accurately convey the meaning and nuances of the original text while adhering to {} grammar, vocabulary, and cultural sensitivities.\nProduce only the {} translation, without any additional explanations or commentary. Please translate the following text into {}:\n\n\n{}",
            full_tl, prompt_tl, full_tl, full_tl, full_tl, text
        )
    } else {
        format!(
            "You are a professional {} ({}) to {} ({}) translator. Your goal is to accurately convey the meaning and nuances of the original {} text while adhering to {} grammar, vocabulary, and cultural sensitivities.\nProduce only the {} translation, without any additional explanations or commentary. Please translate the following {} text into {}:\n\n\n{}",
            full_sl, prompt_sl, full_tl, prompt_tl, full_sl, full_tl, full_tl, full_sl, full_tl, text
        )
    };

    let body = ChatCompletionRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: "user".to_string(),
                content: prompt,
            },
        ],
        temperature: 0.1,
    };

    let max_retries = 10u32;
    let mut attempt = 0u32;
    
    loop {
        let resp_result = client
            .post(url)
            .json(&body)
            .send()
            .await;

        match resp_result {
            Ok(resp) => {
                if resp.status().is_success() {
                    let chat_resp: ChatCompletionResponse = resp.json().await?;
                    if let Some(choice) = chat_resp.choices.get(0) {
                        let translated = choice.message.content.trim().to_string();
                        // Strip any potential quotes the LLM might have added
                        let translated = translated.trim_matches('"').trim_matches('\'').trim().to_string();
                        return Ok(translated);
                    }
                    return Err("No translation choices returned from local AI".into());
                } else if resp.status().is_server_error() || resp.status().as_u16() == 429 {
                    if attempt >= max_retries { break; }
                    sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
                    attempt += 1;
                    continue;
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(format!("Local translation API error ({}): {}", status, body).into());
                }
            }
            Err(e) => {
                println!("DEBUG: Translation attempt {} failed: {}", attempt + 1, e);
                if attempt >= max_retries { 
                    return Err(format!("Failed to connect to local translation service at {} after {} attempts. Ensure your Ollama server is running and the model '{}' is available. Error: {}", url, max_retries, model, e).into()); 
                }
                // Back off slightly longer to give the server time to breathe
                sleep(Duration::from_millis(1000 * (attempt as u64 + 1))).await;
                attempt += 1;
                continue;
            }
        }
    }

    Err("Local translation failed after maximum retries".into())
}

/// Translate a batch of segments in-place.
///
/// - Minimizes number of HTTP requests by batching multiple segments into a single request
///   using a robust delimiter strategy.
/// - Overwrites `segment.text` with the translated text.
/// - Regenerates `segment.words` with evenly interpolated timestamps between `start` and `end`.
pub async fn translate_segments(
    segments: &mut [Segment],
    from: &str,
    to: &str,
    progress: Option<&LabeledProgressFn>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Indices of non-empty segments to translate
    let mut indices: Vec<usize> = Vec::new();
    let mut inputs: Vec<String> = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        let t = seg.text.trim();
        if !t.is_empty() {
            indices.push(i);
            inputs.push(t.to_string());
        }
    }

    if inputs.is_empty() { return Ok(()); }

    // Progress setup
    let total = inputs.len();
    let mut completed: usize = 0;
    let start_label = "progressSteps.translate";
    // Report start at 0%
    if total > 0 {
        if let Some(p) = progress { p(0, ProgressType::Translate, &start_label); }
    }

    // Translate concurrently with bounded concurrency; keep track of original order via enumerate index
    let concurrency: usize = 4;
    let mut out: Vec<Option<String>> = vec![None; total];
    let mut stream = stream::iter(inputs.into_iter().enumerate())
        .map(|(k, txt)| async move { (k, translate_text(&txt, from, to).await) })
        .buffer_unordered(concurrency);

    while let Some((k, res)) = stream.next().await {
        match res {
            Ok(tr) => {
                out[k] = Some(tr);
            }
            Err(e) => {
                println!("ERROR: Translation segment failed: {}", e);
                // Leave as None to keep original text on error
            }
        }
        completed += 1;
        // Incremental progress
        let percent = ((completed as f64) / (total as f64) * 100.0).round() as i32;
        if let Some(p) = progress { p(percent.min(99), ProgressType::Translate, start_label); }
    }

    // Apply results back to segments
    for (k, maybe_tr) in out.into_iter().enumerate() {
        let seg_idx = indices[k];
        if let Some(tr) = maybe_tr {
            let seg = &mut segments[seg_idx];
            seg.text = tr;
            regenerate_words_uniform(seg);
        }
    }

    // Completion progress
    if total > 0 {
        if let Some(p) = progress { p(100, ProgressType::Translate, start_label); }
    }

    Ok(())
}

/// Regenerate `words` for a segment by splitting text on whitespace
/// and interpolating timestamps uniformly between segment.start and segment.end.
/// Words after the first are prefixed with a space so that the formatting layer
/// can reconstruct the original spacing when rendering.
fn regenerate_words_uniform(seg: &mut Segment) {
    // Split on Unicode whitespace; filter out empty tokens
    let tokens: Vec<&str> = seg
        .text
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .collect();

    let n = tokens.len();
    if n == 0 {
        seg.words = Some(Vec::new());
        return;
    }

    let start = seg.start;
    let end = seg.end.max(start); // guard against inverted times
    let dur = end - start;

    // Assign bounds so that words tile the interval [start, end].
    // Prefix words after the first with a space so the formatting layer
    // knows to insert inter-word spacing.
    let mut words = Vec::with_capacity(n);
    for (i, w) in tokens.into_iter().enumerate() {
        let t0 = start + dur * (i as f64) / (n as f64);
        let t1 = start + dur * ((i + 1) as f64) / (n as f64);
        let text = if i == 0 { w.to_string() } else { format!(" {}", w) };
        words.push(WordTimestamp { text, start: t0, end: t1, probability: None });
    }

    seg.words = Some(words);
}

// get_translate_languages moved to utils.rs

fn get_language_info<'a>(code: &'a str) -> (&'static str, &'a str) {
    match code.to_lowercase().as_str() {
        "af" => ("Afrikaans", "af"),
        "sq" => ("Albanian", "sq"),
        "am" => ("Amharic", "am"),
        "ar" => ("Arabic", "ar"),
        "hy" => ("Armenian", "hy"),
        "az" => ("Azerbaijani", "az"),
        "eu" => ("Basque", "eu"),
        "be" => ("Belarusian", "be"),
        "bn" => ("Bengali", "bn"),
        "bs" => ("Bosnian", "bs"),
        "bg" => ("Bulgarian", "bg"),
        "ca" => ("Catalan", "ca"),
        "ceb" => ("Cebuano", "ceb"),
        "ny" => ("Chichewa", "ny"),
        "zh" | "zh-cn" | "zh-hans" => ("Chinese (Simplified)", "zh-Hans"),
        "zh-tw" | "zh-hant" => ("Chinese (Traditional)", "zh-Hant"),
        "co" => ("Corsican", "co"),
        "hr" => ("Croatian", "hr"),
        "cs" => ("Czech", "cs"),
        "da" => ("Danish", "da"),
        "nl" => ("Dutch", "nl"),
        "en" => ("English", "en"),
        "eo" => ("Esperanto", "eo"),
        "et" => ("Estonian", "et"),
        "tl" | "fil" => ("Filipino", "fil-PH"),
        "fi" => ("Finnish", "fi"),
        "fr" => ("French", "fr"),
        "fy" => ("Western Frisian", "fy"),
        "gl" => ("Galician", "gl"),
        "ka" => ("Georgian", "ka"),
        "de" => ("German", "de"),
        "el" => ("Greek", "el"),
        "gu" => ("Gujarati", "gu"),
        "ht" => ("Haitian", "ht"),
        "ha" => ("Hausa", "ha"),
        "haw" => ("Hawaiian", "haw"),
        "he" => ("Hebrew", "he"),
        "hi" => ("Hindi", "hi"),
        "hmn" => ("Hmong", "hmn"),
        "hu" => ("Hungarian", "hu"),
        "is" => ("Icelandic", "is"),
        "ig" => ("Igbo", "ig"),
        "id" => ("Indonesian", "id"),
        "ga" => ("Irish", "ga"),
        "it" => ("Italian", "it"),
        "ja" => ("Japanese", "ja"),
        "jv" | "jw" => ("Javanese", "jv"),
        "kn" => ("Kannada", "kn"),
        "kk" => ("Kazakh", "kk"),
        "km" => ("Central Khmer", "km"),
        "rw" => ("Kinyarwanda", "rw"),
        "ko" => ("Korean", "ko"),
        "ku" => ("Kurdish", "ku"),
        "ky" => ("Kyrgyz", "ky"),
        "lo" => ("Lao", "lo"),
        "la" => ("Latin", "la"),
        "lv" => ("Latvian", "lv"),
        "lt" => ("Lithuanian", "lt"),
        "lb" => ("Luxembourgish", "lb"),
        "mk" => ("Macedonian", "mk"),
        "mg" => ("Malagasy", "mg"),
        "ms" => ("Malay", "ms"),
        "ml" => ("Malayalam", "ml"),
        "mt" => ("Maltese", "mt"),
        "mi" => ("Maori", "mi"),
        "mr" => ("Marathi", "mr"),
        "mn" => ("Mongolian", "mn"),
        "my" => ("Burmese", "my"),
        "ne" => ("Nepali", "ne"),
        "no" | "nb" => ("Norwegian Bokmål", "nb"),
        "nn" => ("Norwegian Nynorsk", "nn"),
        "or" => ("Oriya", "or"),
        "ps" => ("Pashto", "ps"),
        "fa" => ("Persian", "fa"),
        "pl" => ("Polish", "pl"),
        "pt" => ("Portuguese", "pt"),
        "pa" => ("Punjabi", "pa"),
        "ro" => ("Romanian", "ro"),
        "ru" => ("Russian", "ru"),
        "sm" => ("Samoan", "sm"),
        "gd" => ("Scottish Gaelic", "gd"),
        "sr" => ("Serbian", "sr"),
        "st" => ("Southern Sotho", "st"),
        "sn" => ("Shona", "sn"),
        "sd" => ("Sindhi", "sd"),
        "si" => ("Sinhala", "si"),
        "sk" => ("Slovak", "sk"),
        "sl" => ("Slovenian", "sl"),
        "so" => ("Somali", "so"),
        "es" => ("Spanish", "es"),
        "su" => ("Sundanese", "su"),
        "sw" => ("Swahili", "sw"),
        "sv" => ("Swedish", "sv"),
        "tg" => ("Tajik", "tg"),
        "ta" => ("Tamil", "ta"),
        "te" => ("Telugu", "te"),
        "th" => ("Thai", "th"),
        "tr" => ("Turkish", "tr"),
        "uk" => ("Ukrainian", "uk"),
        "ur" => ("Urdu", "ur"),
        "ug" => ("Uyghur", "ug"),
        "uz" => ("Uzbek", "uz"),
        "vi" => ("Vietnamese", "vi"),
        "cy" => ("Welsh", "cy"),
        "xh" => ("Xhosa", "xh"),
        "yi" => ("Yiddish", "yi"),
        "yo" => ("Yoruba", "yo"),
        "zu" => ("Zulu", "zu"),
        _ => ("Unknown", code)
    }
}