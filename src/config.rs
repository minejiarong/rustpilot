use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// LLM提供者类型：ollama, openai, local
    pub provider: String,
    /// API基础URL（用于Ollama）
    pub api_base_url: Option<String>,
    /// API密钥（用于OpenAI）
    pub api_key: Option<String>,
    /// 模型名称
    pub model: String,
    /// 天气API密钥（可选）
    pub weather_api_key: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            provider: "ollama".to_string(),
            api_base_url: Some("http://localhost:11434".to_string()),
            api_key: None,
            model: "llama3.1:8b".to_string(),
            weather_api_key: env::var("WEATHER_API_KEY").ok(),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let provider = env::var("LLM_PROVIDER")
            .unwrap_or_else(|_| "ollama".to_string());
        
        let api_base_url = env::var("OLLAMA_API_BASE")
            .or_else(|_| env::var("OPENAI_API_BASE_URL"))
            .or_else(|_| env::var("DEEPSEEK_API_BASE_URL"))
            .or_else(|_| env::var("REMOTE_MODEL_URL"))
            .ok();
        
        let api_key = env::var("OPENAI_API_KEY")
            .or_else(|_| env::var("DEEPSEEK_API_KEY"))
            .or_else(|_| env::var("REMOTE_MODEL_API_KEY"))
            .ok();
        
        let model = env::var("LLM_MODEL")
            .or_else(|_| env::var("DEEPSEEK_MODEL"))
            .or_else(|_| env::var("REMOTE_MODEL_MODEL"))
            .unwrap_or_else(|_| {
                if provider == "ollama" {
                    "llama3.1:8b".to_string()
                } else if provider == "deepseek" || provider == "remote" {
                    "deepseek-chat".to_string()
                } else {
                    "gpt-3.5-turbo".to_string()
                }
            });
        
        let weather_api_key = env::var("WEATHER_API_KEY").ok();

        Config {
            provider,
            api_base_url,
            api_key,
            model,
            weather_api_key,
        }
    }

    pub fn is_ollama(&self) -> bool {
        self.provider == "ollama"
    }

    pub fn is_openai(&self) -> bool {
        self.provider == "openai"
    }
    
    pub fn is_deepseek(&self) -> bool {
        self.provider == "deepseek"
    }
    
    pub fn is_remote(&self) -> bool {
        self.provider == "remote"
    }
}
