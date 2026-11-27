use serde_json::{json, Value};
use std::time::Duration;

pub const GEMINI_INTERACTIONS_URL: &str =
    "https://generativelanguage.googleapis.com/v1/interactions";

pub const DEFAULT_GEMINI_MODEL: &str = "gemini-4-hydrogen";

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
}

/// Calls the Gemini Interactions REST API (`POST /v1/interactions`).
pub fn create_interaction(
    api_key: &str,
    model: &str,
    input: &str,
    system_instruction: &str,
    previous_interaction_id: Option<&str>,
) -> Result<GeminiReply, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(
            "Gemini API key not set. Open Phone Companion (http://<TV-IP>:8787) to paste your key."
                .to_string(),
        );
    }

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

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(45))
        .build();

    let resp = agent
        .post(GEMINI_INTERACTIONS_URL)
        .set("x-goog-api-key", key)
        .set("Content-Type", "application/json")
        .send_json(body);

    let val: Value = match resp {
        Ok(r) => r
            .into_json()
            .map_err(|e| format!("Failed to parse Gemini JSON: {e}"))?,
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
                .unwrap_or_else(|| {
                    err_body.chars().take(180).collect::<String>()
                });
            return Err(format!("Gemini HTTP {code}: {detail}"));
        }
        Err(ureq::Error::Transport(t)) => {
            return Err(format!("Gemini network error: {t}"));
        }
    };

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
    })
}

/// Ask Gemini a concise TV-friendly question with optional multi-turn memory.
pub fn ask_tv_assistant(
    api_key: &str,
    model: &str,
    prompt: &str,
    previous_interaction_id: Option<&str>,
    tv_context: &str,
) -> Result<GeminiReply, String> {
    let sys = format!(
        "You are Gemini built into Astryx TV Launcher on Android TV. \
         Reply concisely for a 10-foot TV screen using plain text (no markdown tables or heavy formatting), \
         ideally in 2-5 short lines. Context: {tv_context}"
    );
    create_interaction(api_key, model, prompt, &sys, previous_interaction_id)
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
    let reply = create_interaction(api_key, model, prompt, &sys, None)?;
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
