use super::{normalize_reasoning_effort, AiProvider, AiSettings};
use crate::error::AppError;
use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize)]
pub struct AiModelOption {
    pub id: String,
    pub name: String,
}

pub fn default_model_for_provider(provider: &str) -> &'static str {
    match provider {
        "openai" => "gpt-4o-mini",
        "claude" => "claude-3-5-sonnet-latest",
        "openai_compat" => "llama3.2",
        _ => "gemini-2.5-flash",
    }
}

fn derive_models_url(endpoint: &str, default_url: &str) -> String {
    let ep = endpoint.trim().trim_end_matches('/');
    if ep.is_empty() {
        return default_url.to_string();
    }
    if let Some(prefix) = ep.strip_suffix("/chat/completions") {
        return format!("{}/models", prefix);
    }
    if let Some(prefix) = ep.strip_suffix("/messages") {
        return format!("{}/models", prefix);
    }
    if let Some(prefix) = ep.strip_suffix("/api/chat") {
        return format!("{}/api/tags", prefix);
    }
    if let Some(prefix) = ep.strip_suffix("/api/generate") {
        return format!("{}/api/tags", prefix);
    }
    if ep.ends_with("/models") || ep.ends_with("/tags") {
        return ep.to_string();
    }
    format!("{}/models", ep)
}

pub async fn fetch_available_models(
    client: &Client,
    cfg: &AiSettings,
) -> Result<Vec<AiModelOption>, String> {
    let key = cfg.api_key.trim();
    match cfg.provider.as_str() {
        "gemini" => {
            if key.is_empty() {
                return Err("Gemini API Key를 먼저 입력하거나 저장해 주세요.".into());
            }
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}&pageSize=100",
                key
            );
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Gemini 모델 목록 조회 실패: {}", e))?;
            if !resp.status().is_success() {
                let st = resp.status();
                let err = resp.text().await.unwrap_or_default();
                return Err(format!("Gemini 오류 ({}): {}", st, err));
            }
            let val: Value = resp
                .json()
                .await
                .map_err(|e| format!("Gemini 응답 파싱 실패: {}", e))?;
            let mut models = Vec::new();
            if let Some(arr) = val.get("models").and_then(|v| v.as_array()) {
                for m in arr {
                    let supports_gen = m
                        .get("supportedGenerationMethods")
                        .and_then(|v| v.as_array())
                        .map(|methods| {
                            methods
                                .iter()
                                .any(|item| item.as_str() == Some("generateContent"))
                        })
                        .unwrap_or(false);
                    if !supports_gen {
                        continue;
                    }
                    if let Some(raw_name) = m.get("name").and_then(|v| v.as_str()) {
                        let id = raw_name.strip_prefix("models/").unwrap_or(raw_name).to_string();
                        let display = m
                            .get("displayName")
                            .and_then(|v| v.as_str())
                            .map(|d| {
                                if d.eq_ignore_ascii_case(&id) {
                                    id.clone()
                                } else {
                                    format!("{} ({})", id, d)
                                }
                            })
                            .unwrap_or_else(|| id.clone());
                        models.push(AiModelOption { id, name: display });
                    }
                }
            }
            models.sort_by(|a, b| b.id.cmp(&a.id));
            Ok(models)
        }
        "openai" => {
            if key.is_empty() {
                return Err("OpenAI API Key를 먼저 입력하거나 저장해 주세요.".into());
            }
            let url = derive_models_url(&cfg.endpoint, "https://api.openai.com/v1/models");
            let resp = client
                .get(&url)
                .bearer_auth(key)
                .send()
                .await
                .map_err(|e| format!("OpenAI 모델 목록 조회 실패: {}", e))?;
            if !resp.status().is_success() {
                let st = resp.status();
                let err = resp.text().await.unwrap_or_default();
                return Err(format!("OpenAI 오류 ({}): {}", st, err));
            }
            let val: Value = resp
                .json()
                .await
                .map_err(|e| format!("OpenAI 응답 파싱 실패: {}", e))?;
            let mut all_models = Vec::new();
            let mut chat_models = Vec::new();
            if let Some(arr) = val.get("data").and_then(|v| v.as_array()) {
                for m in arr {
                    if let Some(id) = m.get("id").and_then(|v| v.as_str()) {
                        let item = AiModelOption {
                            id: id.to_string(),
                            name: id.to_string(),
                        };
                        let lower = id.to_ascii_lowercase();
                        let is_chat_prefix = lower.starts_with("gpt-")
                            || lower.starts_with("o1")
                            || lower.starts_with("o3")
                            || lower.starts_with("o4")
                            || lower.starts_with("chatgpt-");
                        let is_excluded = lower.contains("realtime")
                            || lower.contains("audio")
                            || lower.contains("transcribe")
                            || lower.contains("tts")
                            || lower.contains("instruct")
                            || lower.contains("search");
                        if is_chat_prefix && !is_excluded {
                            chat_models.push(item.clone());
                        }
                        all_models.push(item);
                    }
                }
            }
            let mut out = if chat_models.is_empty() {
                all_models
            } else {
                chat_models
            };
            out.sort_by(|a, b| b.id.cmp(&a.id));
            Ok(out)
        }
        "claude" => {
            if key.is_empty() {
                return Err("Anthropic Claude API Key를 먼저 입력하거나 저장해 주세요.".into());
            }
            let base = derive_models_url(&cfg.endpoint, "https://api.anthropic.com/v1/models");
            let url = if base.contains('?') {
                base
            } else {
                format!("{}?limit=100", base)
            };
            let resp = client
                .get(&url)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .send()
                .await
                .map_err(|e| format!("Claude 모델 목록 조회 실패: {}", e))?;
            if !resp.status().is_success() {
                let st = resp.status();
                let err = resp.text().await.unwrap_or_default();
                return Err(format!("Claude 오류 ({}): {}", st, err));
            }
            let val: Value = resp
                .json()
                .await
                .map_err(|e| format!("Claude 응답 파싱 실패: {}", e))?;
            let mut models = Vec::new();
            if let Some(arr) = val.get("data").and_then(|v| v.as_array()) {
                for m in arr {
                    if let Some(id) = m.get("id").and_then(|v| v.as_str()) {
                        let display = m
                            .get("display_name")
                            .and_then(|v| v.as_str())
                            .map(|d| format!("{} ({})", id, d))
                            .unwrap_or_else(|| id.to_string());
                        models.push(AiModelOption {
                            id: id.to_string(),
                            name: display,
                        });
                    }
                }
            }
            Ok(models)
        }
        _ => {
            let ep = if cfg.endpoint.trim().is_empty() {
                "http://127.0.0.1:11434/v1/chat/completions"
            } else {
                cfg.endpoint.trim()
            };
            let url = derive_models_url(ep, "http://127.0.0.1:11434/v1/models");
            let mut req = client.get(&url);
            if !key.is_empty() {
                req = req.bearer_auth(key);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| format!("로컬/호환 서버 모델 목록 조회 실패 ({}): {}", url, e))?;
            if !resp.status().is_success() {
                let st = resp.status();
                let err = resp.text().await.unwrap_or_default();
                return Err(format!("모델 목록 조회 오류 ({}): {}", st, err));
            }
            let val: Value = resp
                .json()
                .await
                .map_err(|e| format!("응답 파싱 실패: {}", e))?;
            let mut models = Vec::new();
            if let Some(arr) = val.get("data").and_then(|v| v.as_array()) {
                for m in arr {
                    if let Some(id) = m.get("id").and_then(|v| v.as_str()) {
                        models.push(AiModelOption {
                            id: id.to_string(),
                            name: id.to_string(),
                        });
                    }
                }
            } else if let Some(arr) = val.get("models").and_then(|v| v.as_array()) {
                for m in arr {
                    if let Some(id) = m.get("name").and_then(|v| v.as_str()) {
                        models.push(AiModelOption {
                            id: id.to_string(),
                            name: id.to_string(),
                        });
                    }
                }
            }
            Ok(models)
        }
    }
}

fn extract_json_block(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find("```json") {
        let rest = &trimmed[start + 7..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim().to_string();
        }
    }
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end >= start {
                return trimmed[start..=end].to_string();
            }
        }
    }
    trimmed.to_string()
}

pub struct GeminiAiProvider;

#[async_trait]
impl AiProvider for GeminiAiProvider {
    async fn generate_plan(
        &self,
        client: &Client,
        cfg: &AiSettings,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, AppError> {
        if cfg.api_key.trim().is_empty() {
            return Err(AppError::BadRequest("Gemini API Key가 설정되지 않았습니다.".into()));
        }
        let model = if cfg.model.trim().is_empty() {
            "gemini-2.5-flash"
        } else {
            cfg.model.trim()
        };
        let url = if cfg.endpoint.trim().is_empty() {
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                model,
                cfg.api_key.trim()
            )
        } else {
            cfg.endpoint.trim().to_string()
        };

        let effort = normalize_reasoning_effort(&cfg.reasoning_effort);
        let mut generation_config = json!({ "responseMimeType": "application/json" });
        if effort != "default" {
            let lower = model.to_ascii_lowercase();
            if lower.contains("gemini-2.5") || lower.contains("gemini-2.0") {
                let budget = match effort.as_str() {
                    "minimal" => 512,
                    "low" => 1024,
                    "medium" => 8192,
                    "high" => 24576,
                    _ => -1,
                };
                generation_config["thinkingConfig"] = json!({ "thinkingBudget": budget });
            } else {
                generation_config["thinkingConfig"] = json!({ "thinkingLevel": effort });
            }
        }

        let body = json!({
            "systemInstruction": { "parts": [{ "text": system_prompt }] },
            "contents": [{ "role": "user", "parts": [{ "text": user_prompt }] }],
            "generationConfig": generation_config
        });

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Gemini API 요청 실패: {}", e)))?;

        if !resp.status().is_success() {
            let st = resp.status();
            let err = resp.text().await.unwrap_or_default();
            return Err(AppError::BadRequest(format!("Gemini 오류 ({}): {}", st, err)));
        }

        let json_val: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Gemini 응답 파싱 실패: {}", e)))?;
        let text = json_val
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Internal("Gemini 응답에서 텍스트를 추출하지 못했습니다.".into()))?;

        Ok(extract_json_block(text))
    }
}

pub struct OpenAiProvider {
    pub compat_mode: bool,
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    async fn generate_plan(
        &self,
        client: &Client,
        cfg: &AiSettings,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, AppError> {
        if !self.compat_mode && cfg.api_key.trim().is_empty() {
            return Err(AppError::BadRequest("OpenAI API Key가 설정되지 않았습니다.".into()));
        }
        let url = if cfg.endpoint.trim().is_empty() {
            "https://api.openai.com/v1/chat/completions".to_string()
        } else {
            cfg.endpoint.trim().to_string()
        };
        let model = if cfg.model.trim().is_empty() {
            "gpt-4o-mini"
        } else {
            cfg.model.trim()
        };

        let effort = normalize_reasoning_effort(&cfg.reasoning_effort);
        let mut body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ]
        });
        if effort != "default" {
            body["reasoning_effort"] = json!(effort);
        } else {
            let lower = model.to_ascii_lowercase();
            if !lower.starts_with("o1") && !lower.starts_with("o3") && !lower.starts_with("o4") {
                body["temperature"] = json!(0.2);
            }
        }

        let mut req = client.post(&url).json(&body);
        if !cfg.api_key.trim().is_empty() {
            req = req.bearer_auth(cfg.api_key.trim());
        }

        let resp = req
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("OpenAI 요청 실패: {}", e)))?;

        if !resp.status().is_success() {
            let st = resp.status();
            let err = resp.text().await.unwrap_or_default();
            return Err(AppError::BadRequest(format!("OpenAI 오류 ({}): {}", st, err)));
        }

        let json_val: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("OpenAI 응답 파싱 실패: {}", e)))?;
        let text = json_val
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Internal("OpenAI 응답 content가 비어 있습니다.".into()))?;

        Ok(extract_json_block(text))
    }
}

pub struct ClaudeAiProvider;

#[async_trait]
impl AiProvider for ClaudeAiProvider {
    async fn generate_plan(
        &self,
        client: &Client,
        cfg: &AiSettings,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, AppError> {
        if cfg.api_key.trim().is_empty() {
            return Err(AppError::BadRequest("Anthropic Claude API Key가 설정되지 않았습니다.".into()));
        }
        let url = if cfg.endpoint.trim().is_empty() {
            "https://api.anthropic.com/v1/messages".to_string()
        } else {
            cfg.endpoint.trim().to_string()
        };
        let model = if cfg.model.trim().is_empty() {
            "claude-3-5-sonnet-latest"
        } else {
            cfg.model.trim()
        };

        let effort = normalize_reasoning_effort(&cfg.reasoning_effort);
        let mut body = json!({
            "model": model,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": [{ "role": "user", "content": user_prompt }]
        });
        if effort != "default" {
            let lower = model.to_ascii_lowercase();
            if lower.contains("4-6") || lower.contains("4.6") || lower.contains("-5") || lower.contains("5.") {
                let mapped_effort = if effort == "minimal" { "low" } else { effort.as_str() };
                body["max_tokens"] = json!(16000);
                body["thinking"] = json!({ "type": "adaptive" });
                body["output_config"] = json!({ "effort": mapped_effort });
            } else if lower.contains("3-7")
                || lower.contains("3.7")
                || lower.contains("-4")
                || lower.contains("4-")
                || lower.contains("4.")
            {
                let budget: u64 = match effort.as_str() {
                    "minimal" => 1024,
                    "low" => 2048,
                    "medium" => 8192,
                    "high" => 16384,
                    _ => 2048,
                };
                body["max_tokens"] = json!(budget + 4096);
                body["thinking"] = json!({ "type": "enabled", "budget_tokens": budget });
            }
        }

        let resp = client
            .post(&url)
            .header("x-api-key", cfg.api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Claude API 요청 실패: {}", e)))?;

        if !resp.status().is_success() {
            let st = resp.status();
            let err = resp.text().await.unwrap_or_default();
            return Err(AppError::BadRequest(format!("Claude 오류 ({}): {}", st, err)));
        }

        let json_val: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Claude 응답 파싱 실패: {}", e)))?;
        let text = json_val
            .get("content")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .or_else(|| arr.iter().find(|item| item.get("text").is_some()))
            })
            .and_then(|item| item.get("text"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Internal("Claude 응답 text가 비어 있습니다.".into()))?;

        Ok(extract_json_block(text))
    }
}

#[cfg(test)]
mod tests {
    use super::{default_model_for_provider, derive_models_url};

    #[test]
    fn test_default_model_for_provider() {
        assert_eq!(default_model_for_provider("gemini"), "gemini-2.5-flash");
        assert_eq!(default_model_for_provider("openai"), "gpt-4o-mini");
        assert_eq!(default_model_for_provider("claude"), "claude-3-5-sonnet-latest");
        assert_eq!(default_model_for_provider("openai_compat"), "llama3.2");
    }

    #[test]
    fn test_derive_models_url() {
        assert_eq!(
            derive_models_url("", "https://api.openai.com/v1/models"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            derive_models_url("http://127.0.0.1:11434/v1/chat/completions", ""),
            "http://127.0.0.1:11434/v1/models"
        );
        assert_eq!(
            derive_models_url("http://127.0.0.1:11434/api/chat", ""),
            "http://127.0.0.1:11434/api/tags"
        );
    }
}
