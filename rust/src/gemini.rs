use serde_json::{json, Value};
use std::time::Duration;

pub const GEMINI_INTERACTIONS_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/interactions";

pub const DEFAULT_GEMINI_MODEL: &str = "gemini-4-hydrogen";
pub const GEMINI_TTS_MODEL: &str = "gemini-3.8-flash-lite-tts";

pub const GEMINI_MODELS: [&str; 3] = [
    "gemini-4-hydrogen",
    "gemini-4-neon",
    "gemini-3.5-flash-lite",
];

pub fn next_model(current: &str) -> &'static str {
    let idx = GEMINI_MODELS
        .iter()
        .position(|&m| m == current)
        .map(|i| (i + 1) % GEMINI_MODELS.len())
        .unwrap_or(0);
    GEMINI_MODELS[idx]
}

#[derive(Debug, Clone)]
pub struct GeminiReply {
    pub interaction_id: String,
    pub text: String,
    pub tts_wav: Option<Vec<u8>>,
}

/// Calls the Gemini Interactions REST API (`POST /v1beta/interactions`) with arbitrary `input` JSON.
pub fn post_interaction_json(api_key: &str, body: Value) -> Result<Value, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(
            "Gemini API key not set. Open Phone Companion (http://<TV-IP>:8787) to paste your key."
                .to_string(),
        );
    }

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(45))
        .build();

    let resp = agent
        .post(GEMINI_INTERACTIONS_URL)
        .set("x-goog-api-key", key)
        .set("Content-Type", "application/json")
        .send_json(body);

    match resp {
        Ok(r) => r
            .into_json()
            .map_err(|e| format!("Failed to parse Gemini JSON: {e}")),
        Err(ureq::Error::Status(code, r)) => {
            let err_body = r.into_string().unwrap_or_default();
            let detail = serde_json::from_str::<Value>(&err_body)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| err_body.chars().take(180).collect::<String>());
            Err(format!("Gemini HTTP {code}: {detail}"))
        }
        Err(ureq::Error::Transport(t)) => Err(format!("Gemini network error: {t}")),
    }
}

pub fn create_interaction(
    api_key: &str,
    model: &str,
    input: Value,
    system_instruction: &str,
    previous_interaction_id: Option<&str>,
) -> Result<GeminiReply, String> {
    let model_name = if model.trim().is_empty() {
        DEFAULT_GEMINI_MODEL
    } else {
        model.trim()
    };

    let mut body = json!({
        "model": model_name,
        "input": input,
        "system_instruction": system_instruction,
    });

    if let Some(prev_id) = previous_interaction_id.filter(|s| !s.trim().is_empty()) {
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "previous_interaction_id".to_string(),
                Value::String(prev_id.to_string()),
            );
        }
    }

    let val = post_interaction_json(api_key, body)?;

    let interaction_id = val
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let text = extract_output_text(&val)
        .ok_or_else(|| "Gemini returned no text output.".to_string())?;

    Ok(GeminiReply {
        interaction_id,
        text,
        tts_wav: None,
    })
}

/// Synthesizes expressive spoken WAV audio using `gemini-3.8-flash-lite-tts`.
pub fn synthesize_speech_wav(api_key: &str, text: &str) -> Option<Vec<u8>> {
    let spoken = text.trim();
    if spoken.is_empty() || api_key.trim().is_empty() {
        return None;
    }
    let body = json!({
        "model": GEMINI_TTS_MODEL,
        "input": [{
            "type": "user_input",
            "content": [{
                "type": "text",
                "text": spoken,
                "annotations": [{
                    "type": "speech_metadata",
                    "style": "warm, natural, friendly conversational TV assistant"
                }]
            }]
        }],
        "response_format": { "type": "audio" },
        "generation_config": {
            "speech_config": [
                { "voice": "Puck" }
            ]
        }
    });

    let val = post_interaction_json(api_key, body).ok()?;
    extract_output_audio_bytes(&val)
}

/// Ask Gemini a concise TV-friendly question (with optional spoken TTS synthesis).
pub fn ask_tv_assistant(
    api_key: &str,
    model: &str,
    prompt: &str,
    previous_interaction_id: Option<&str>,
    tv_context: &str,
    with_tts: bool,
) -> Result<GeminiReply, String> {
    let sys = format!(
        "You are Gemini Voice Assistant built into Astryx TV Launcher on Android TV. \
         Reply naturally in conversational spoken sentences (plain text only, no bullet symbols, \
         no markdown asterisks or tables), ideally 1 to 3 concise sentences suitable to read aloud on TV. \
         Context: {tv_context}"
    );
    let mut reply = create_interaction(
        api_key,
        model,
        Value::String(prompt.to_string()),
        &sys,
        previous_interaction_id,
    )?;
    if with_tts {
        reply.tts_wav = synthesize_speech_wav(api_key, &reply.text);
    }
    Ok(reply)
}

/// Send recorded user speech WAV (`audio/wav`) directly to Gemini and synthesize a spoken WAV reply.
pub fn ask_tv_voice_assistant(
    api_key: &str,
    model: &str,
    wav_bytes: &[u8],
    mime_type: &str,
    previous_interaction_id: Option<&str>,
    tv_context: &str,
) -> Result<GeminiReply, String> {
    if wav_bytes.len() < 800 {
        return Err("No clear speech detected — speak closer to the mic and try again.".to_string());
    }
    let b64_audio = base64_encode(wav_bytes);
    let sys = format!(
        "You are Gemini Voice Assistant built into Astryx TV Launcher on Android TV. \
         The user is speaking aloud to their TV. Listen carefully to their spoken audio and answer \
         directly in warm, natural, conversational plain text (no markdown symbols, asterisks, or bullet lists) \
         in 1 to 3 concise sentences ready to be spoken aloud. Context: {tv_context}"
    );
    let input = json!([
        {
            "type": "text",
            "text": "Listen to the user's spoken request and reply naturally."
        },
        {
            "type": "audio",
            "data": b64_audio,
            "mime_type": mime_type
        }
    ]);
    let mut reply = create_interaction(api_key, model, input, &sys, previous_interaction_id)?;
    reply.tts_wav = synthesize_speech_wav(api_key, &reply.text);
    Ok(reply)
}

/// Ask Gemini to generate an A2UI v1.0 JSON payload for native rendering on the TV home screen.
pub fn generate_a2ui_surface(
    api_key: &str,
    model: &str,
    prompt: &str,
    tv_context: &str,
) -> Result<String, String> {
    let sys = format!(
        "You generate ONLY raw valid JSON (no markdown code fences, no prose) representing an A2UI v1.0 \
         message array for Astryx TV Launcher. Return a JSON array with two objects: \
         1) {{\"version\":\"1.0\",\"createSurface\":{{\"surfaceId\":\"gemini_surface\",\"catalogId\":\"https://astryxtv.local/catalogs/tv-v1.json\"}}}} \
         2) {{\"version\":\"1.0\",\"updateComponents\":{{\"surfaceId\":\"gemini_surface\",\"components\":[...]}}}} \
         Components must include one root TvShelf ({{\"id\":\"root\",\"component\":\"TvShelf\",\"title\":\"...\",\"children\":[\"c1\",\"c2\",\"c3\"]}}) \
         and 2 to 4 TvWidgetCard items ({{\"id\":\"c1\",\"component\":\"TvWidgetCard\",\"title\":\"...\",\"value\":\"...\",\"subtitle\":\"...\"}}). \
         Keep titles, values, and subtitles crisp and TV-friendly. Context: {tv_context}"
    );
    let reply = create_interaction(
        api_key,
        model,
        Value::String(prompt.to_string()),
        &sys,
        None,
    )?;
    Ok(strip_json_fences(&reply.text))
}

fn extract_output_text(val: &Value) -> Option<String> {
    let steps = val.get("steps")?.as_array()?;
    let mut trailing_chunks: Vec<String> = Vec::new();
    for step in steps.iter().rev() {
        let step_type = step.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if step_type != "model_output" {
            if !trailing_chunks.is_empty() {
                break;
            }
            continue;
        }
        if let Some(parts) = step.get("content").and_then(|c| c.as_array()) {
            let mut step_text = String::new();
            for part in parts {
                if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                        step_text.push_str(t);
                    }
                }
            }
            if !step_text.is_empty() {
                trailing_chunks.push(step_text);
            }
        }
    }
    if trailing_chunks.is_empty() {
        return None;
    }
    trailing_chunks.reverse();
    Some(trailing_chunks.join("\n"))
}

fn extract_output_audio_bytes(val: &Value) -> Option<Vec<u8>> {
    let steps = val.get("steps")?.as_array()?;
    for step in steps.iter().rev() {
        if step.get("type").and_then(|t| t.as_str()) == Some("model_output") {
            if let Some(parts) = step.get("content").and_then(|c| c.as_array()) {
                for part in parts.iter().rev() {
                    if part.get("type").and_then(|t| t.as_str()) == Some("audio") {
                        if let Some(b64) = part.get("data").and_then(|d| d.as_str()) {
                            if let Some(bytes) = base64_decode(b64) {
                                if !bytes.is_empty() {
                                    return Some(bytes);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn strip_json_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let without_lang = rest
            .strip_prefix("json")
            .or_else(|| rest.strip_prefix("JSON"))
            .unwrap_or(rest)
            .trim_start_matches(['\r', '\n']);
        if let Some(body) = without_lang.strip_suffix("```") {
            return body.trim().to_string();
        }
    }
    trimmed.to_string()
}

const B64_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u8;
    for b in s.bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}
