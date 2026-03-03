use async_trait::async_trait;
use llm_chain::tools::{Describe, Format, ToolDescription, Tool, ToolError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 天气查询工具，通过API获取天气信息
pub struct WeatherTool {
    api_key: Option<String>,
}

impl WeatherTool {
    pub fn new(api_key: Option<String>) -> Self {
        WeatherTool { api_key }
    }
}

#[derive(Serialize, Deserialize)]
pub struct WeatherInput {
    /// 城市名称，例如 "北京" 或 "Beijing"
    city: String,
    /// 国家代码（可选），例如 "CN"
    country: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct WeatherOutput {
    /// 城市名称
    city: String,
    /// 当前温度（摄氏度）
    temperature: Option<f64>,
    /// 天气描述
    description: String,
    /// 湿度（百分比）
    humidity: Option<u8>,
    /// 风速（米/秒）
    wind_speed: Option<f64>,
    /// 错误信息（如果有）
    error: Option<String>,
}

impl Describe for WeatherInput {
    fn describe() -> Format {
        vec![
            ("city", "城市名称，例如 '北京' 或 'Beijing'").into(),
            ("country", "国家代码（可选），例如 'CN'").into(),
        ]
        .into()
    }
}

impl Describe for WeatherOutput {
    fn describe() -> Format {
        vec![
            ("city", "查询的城市名称").into(),
            ("temperature", "当前温度（摄氏度）").into(),
            ("description", "天气描述").into(),
            ("humidity", "湿度（百分比）").into(),
            ("wind_speed", "风速（米/秒）").into(),
            ("error", "错误信息（如果有）").into(),
        ]
        .into()
    }
}

#[derive(Debug, Error)]
pub enum WeatherError {
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error("API请求失败: {0}")]
    ApiError(String),
}

impl ToolError for WeatherError {}

#[async_trait]
impl Tool for WeatherTool {
    type Input = WeatherInput;
    type Output = WeatherOutput;
    type Error = WeatherError;

    async fn invoke_typed(&self, input: &WeatherInput) -> Result<WeatherOutput, WeatherError> {
        // 使用高德天气API
        // 如果没有API key，返回模拟数据
        if self.api_key.is_none() {
            return Ok(WeatherOutput {
                city: input.city.clone(),
                temperature: Some(22.0),
                description: "晴天，适合外出".to_string(),
                humidity: Some(65),
                wind_speed: Some(3.5),
                error: Some("未配置API密钥，返回模拟数据。请设置天气API密钥以获取真实天气数据".to_string()),
            });
        }

        let api_key = self.api_key.as_ref().unwrap();
        let city = &input.city;

        // 1. 首先通过地理编码API获取城市编码
        let geocode_url = format!(
            "https://restapi.amap.com/v3/geocode/geo?address={}&key={}",
            city, api_key
        );

        let geocode_response = reqwest::get(&geocode_url).await?;
        if !geocode_response.status().is_success() {
            return Ok(WeatherOutput {
                city: city.clone(),
                temperature: None,
                description: "无法获取天气信息".to_string(),
                humidity: None,
                wind_speed: None,
                error: Some(format!("地理编码API返回错误: {}", geocode_response.status())),
            });
        }

        let geocode_json: serde_json::Value = geocode_response.json().await?;
        
        // 解析城市编码
        let city_code = match geocode_json["geocodes"].as_array() {
            Some(arr) if !arr.is_empty() => {
                match arr[0]["adcode"].as_str() {
                    Some(code) => code.to_string(),
                    None => {
                        return Ok(WeatherOutput {
                            city: city.clone(),
                            temperature: None,
                            description: "无法获取天气信息".to_string(),
                            humidity: None,
                            wind_speed: None,
                            error: Some("无法解析城市编码".to_string()),
                        });
                    }
                }
            },
            _ => {
                return Ok(WeatherOutput {
                    city: city.clone(),
                    temperature: None,
                    description: "无法获取天气信息".to_string(),
                    humidity: None,
                    wind_speed: None,
                    error: Some(format!("未找到城市: {}", city).to_string()),
                });
            }
        };

        // 2. 使用城市编码查询天气
        let weather_url = format!(
            "https://restapi.amap.com/v3/weather/weatherInfo?city={}&key={}&extensions=base",
            city_code, api_key
        );

        let weather_response = reqwest::get(&weather_url).await?;
        
        if !weather_response.status().is_success() {
            return Ok(WeatherOutput {
                city: city.clone(),
                temperature: None,
                description: "无法获取天气信息".to_string(),
                humidity: None,
                wind_speed: None,
                error: Some(format!("天气API返回错误: {}", weather_response.status())),
            });
        }

        let weather_json: serde_json::Value = weather_response.json().await?;
        
        // 解析天气数据
        let weather_data = match weather_json["lives"].as_array() {
            Some(arr) if !arr.is_empty() => arr[0].clone(),
            _ => {
                return Ok(WeatherOutput {
                    city: city.clone(),
                    temperature: None,
                    description: "无法获取天气信息".to_string(),
                    humidity: None,
                    wind_speed: None,
                    error: Some("未找到天气数据".to_string()),
                });
            }
        };

        let temperature = weather_data["temperature"].as_str()
            .and_then(|s| s.parse::<f64>().ok());
        let humidity = weather_data["humidity"].as_str()
            .and_then(|s| s.parse::<u8>().ok());
        let wind_speed = weather_data["windpower"].as_str()
            .and_then(|s| s.parse::<f64>().ok());
        let description = format!("{}，温度{}℃", 
            weather_data["weather"].as_str().unwrap_or("未知"),
            temperature.unwrap_or(0.0)
        );

        Ok(WeatherOutput {
            city: city.clone(),
            temperature,
            description,
            humidity,
            wind_speed,
            error: None,
        })
    }

    fn description(&self) -> ToolDescription {
        ToolDescription::new(
            "WeatherTool",
            "查询指定城市的天气信息，包括温度、湿度、风速等",
            "使用此工具来查询天气，例如查询北京的天气",
            WeatherInput::describe(),
            WeatherOutput::describe(),
        )
    }
}
