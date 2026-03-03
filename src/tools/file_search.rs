use async_trait::async_trait;
use llm_chain::tools::{Describe, Format, ToolDescription, Tool, ToolError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, Duration};
use chrono::{DateTime, Local};
use thiserror::Error;
use walkdir::WalkDir;

/// 文件搜索工具，可以根据时间、名称等条件搜索文件
pub struct FileSearchTool {}

impl FileSearchTool {
    pub fn new() -> Self {
        FileSearchTool {}
    }
}

impl Default for FileSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Deserialize)]
pub struct FileSearchInput {
    /// 搜索路径（可选，默认为当前工作目录）
    path: Option<String>,
    /// 文件名模式（支持部分匹配，不区分大小写）
    pattern: Option<String>,
    /// 搜索最近N天的文件（例如：1表示昨天到今天）
    days: Option<u64>,
    /// 文件扩展名过滤（例如：["doc", "docx", "pdf"]）
    extensions: Option<Vec<String>>,
    /// 结果数量限制，默认为 300
    limit: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct FileSearchOutput {
    /// 找到的文件列表
    files: Vec<FileInfo>,
    /// 文件总数
    count: usize,
    /// 是否达到了限制
    is_truncated: bool,
    /// 搜索耗时（毫秒）
    search_duration_ms: u64,
}

#[derive(Serialize, Deserialize)]
pub struct FileInfo {
    /// 文件路径
    path: String,
    /// 文件大小（字节）
    size: u64,
    /// 最后修改时间
    modified: String,
    /// 文件类型
    file_type: String,
}

impl Describe for FileSearchInput {
    fn describe() -> Format {
        vec![
            ("path", "搜索路径，默认为当前工作目录").into(),
            ("pattern", "文件名模式（关键字），支持部分匹配").into(),
            ("days", "搜索最近N天的文件").into(),
            ("extensions", "文件扩展名列表，如 ['doc', 'pdf']").into(),
            ("limit", "结果数量限制，默认为 300").into(),
        ]
        .into()
    }
}

impl Describe for FileSearchOutput {
    fn describe() -> Format {
        vec![
            ("files", "找到的文件列表").into(),
            ("count", "文件总数").into(),
            ("is_truncated", "结果是否因为达到 limit 而被截断").into(),
            ("search_duration_ms", "搜索耗时（毫秒）").into(),
        ]
        .into()
    }
}

#[derive(Debug, Error)]
pub enum FileSearchError {
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error("无效的时间范围: {0}")]
    InvalidTimeRange(String),
}

impl ToolError for FileSearchError {}

#[async_trait]
impl Tool for FileSearchTool {
    type Input = FileSearchInput;
    type Output = FileSearchOutput;
    type Error = FileSearchError;

    async fn invoke_typed(&self, input: &FileSearchInput) -> Result<FileSearchOutput, FileSearchError> {
        let start_time = std::time::Instant::now();
        let search_path = input.path.as_ref()
            .map(|p| PathBuf::from(p))
            .unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            });

        let cutoff_time = if let Some(days) = input.days {
            SystemTime::now()
                .checked_sub(Duration::from_secs(days * 24 * 60 * 60))
                .ok_or_else(|| FileSearchError::InvalidTimeRange(format!("无法计算{}天前的时间", days)))?
        } else {
            SystemTime::UNIX_EPOCH
        };

        let limit = input.limit.unwrap_or(300);
        let pattern = input.pattern.as_ref().map(|p| p.to_lowercase());
        let extensions = input.extensions.as_ref().map(|exts| {
            exts.iter().map(|e| e.to_lowercase()).collect::<Vec<_>>()
        });

        let mut found_files = Vec::new();
        let mut total_count = 0;

        for entry in WalkDir::new(&search_path)
            .into_iter()
            .filter_map(|e| e.ok()) 
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            // 1. 时间过滤
            let modified = match metadata.modified() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if modified < cutoff_time {
                continue;
            }

            // 2. 模式匹配 (文件名包含关键字)
            if let Some(ref p) = pattern {
                let file_name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                if !file_name.contains(p) {
                    continue;
                }
            }

            // 3. 扩展名匹配
            if let Some(ref exts) = extensions {
                let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                if !exts.contains(&ext) {
                    continue;
                }
            }

            total_count += 1;

            if found_files.len() < limit {
                let modified_time = DateTime::<Local>::from(modified);
                found_files.push(FileInfo {
                    path: path.to_string_lossy().to_string(),
                    size: metadata.len(),
                    modified: modified_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                    file_type: "file".to_string(),
                });
            }
        }

        // 按修改时间排序（最新的在前）
        found_files.sort_by(|a, b| b.modified.cmp(&a.modified));

        let duration = start_time.elapsed();

        Ok(FileSearchOutput {
            count: total_count,
            is_truncated: total_count > limit,
            files: found_files,
            search_duration_ms: duration.as_millis() as u64,
        })
    }

    fn description(&self) -> ToolDescription {
        ToolDescription::new(
            "FileSearchTool",
            "使用 walkdir 递归搜索文件。支持关键字、日期范围和扩展名过滤。",
            "默认为当前目录。结果会自动按最新修改时间排序。结果数量默认限制为 300。请优先使用正斜杠 '/'。",
            FileSearchInput::describe(),
            FileSearchOutput::describe(),
        )
    }
}
