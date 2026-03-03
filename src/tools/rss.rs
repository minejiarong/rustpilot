use async_trait::async_trait;
use llm_chain::tools::{Describe, Format, ToolDescription, Tool, ToolError};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use rss::Channel;

/// RSS新闻读取工具，通过RSS订阅源获取新闻信息
pub struct RssTool {}

impl RssTool {
    pub fn new() -> Self {
        RssTool {}
    }
}

#[derive(Serialize, Deserialize)]
pub struct RssInput {
    /// RSS订阅源URL（可选），默认为 Sky News World。支持简写：skynews_world, bbc_world
    url: Option<String>,
    /// 要获取的新闻数量（可选），默认为10条
    limit: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct NewsItem {
    /// 新闻标题
    title: String,
    /// 新闻链接
    link: String,
    /// 新闻描述
    description: String,
    /// 发布时间
    pub_date: Option<String>,
    /// 新闻作者
    author: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RssOutput {
    /// 新闻源标题
    channel_title: String,
    /// 新闻源描述
    channel_description: String,
    /// 新闻列表
    news_items: Vec<NewsItem>,
    /// 错误信息（如果有）
    error: Option<String>,
}

impl Describe for RssInput {
    fn describe() -> Format {
        vec![
            ("url", "RSS订阅源URL或简写（可选），默认为 'skynews_world'").into(),
            ("limit", "要获取的新闻数量（可选），默认为10条").into(),
        ]
        .into()
    }
}

impl Describe for RssOutput {
    fn describe() -> Format {
        vec![
            ("channel_title", "新闻源标题").into(),
            ("channel_description", "新闻源描述").into(),
            ("news_items", "新闻列表，包含标题、链接、描述等信息").into(),
            ("error", "错误信息（如果有）").into(),
        ]
        .into()
    }
}

#[derive(Debug, Error)]
pub enum RssError {
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
    #[error(transparent)]
    RssParseError(#[from] rss::Error),
    #[error("请求失败: {0}")]
    RequestError(String),
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
}

impl ToolError for RssError {}

#[async_trait]
impl Tool for RssTool {
    type Input = RssInput;
    type Output = RssOutput;
    type Error = RssError;

    async fn invoke_typed(&self, input: &RssInput) -> Result<RssOutput, RssError> {
        // 定义预置源
        let sources = [
            ("skynews_world", "https://feeds.skynews.com/feeds/rss/world.xml"),
            ("bbc_world", "https://feeds.bbci.co.uk/news/world/rss.xml"),
        ];

        let input_url = input.url.as_deref().unwrap_or("skynews_world");

        // 检查是否使用了简写
        let target_url = sources
            .iter()
            .find(|(name, _)| *name == input_url)
            .map(|(_, url)| url.to_string())
            .unwrap_or_else(|| input_url.to_string());

        // 获取RSS订阅源内容
        let response = reqwest::get(&target_url).await?;
        
        if !response.status().is_success() {
            return Err(RssError::RequestError(format!("HTTP请求失败: {}", response.status())));
        }

        let content = response.text().await?;
        
        // 解析RSS内容
        let channel = Channel::read_from(content.as_bytes())?;
        
        // 转换为新闻列表
        let limit = input.limit.unwrap_or(10);
        let news_items = channel.items
            .iter()
            .take(limit as usize)
            .map(|item| NewsItem {
                title: item.title.clone().unwrap_or_else(|| "无标题".to_string()),
                link: item.link.clone().unwrap_or_else(|| "".to_string()),
                description: item.description.clone().unwrap_or_else(|| "无描述".to_string()),
                pub_date: item.pub_date.clone(),
                author: item.author.clone(),
            })
            .collect();

        Ok(RssOutput {
            channel_title: channel.title,
            channel_description: channel.description,
            news_items,
            error: None,
        })
    }

    fn description(&self) -> ToolDescription {
        ToolDescription::new(
            "RssTool",
            "通过RSS订阅源获取新闻信息。默认使用 Sky News World。支持简写：skynews_world, bbc_world",
            "使用此工具来读取RSS新闻。可以直接输入简写名、完整URL或留空使用默认源。",
            RssInput::describe(),
            RssOutput::describe(),
        )
    }
}