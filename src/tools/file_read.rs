use async_trait::async_trait;
use llm_chain::tools::{Describe, Format, ToolDescription, Tool, ToolError};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use memmap2::MmapOptions;
use thiserror::Error;

/// 文件内容读取工具，支持小文件和大文件的高效读取
pub struct FileReadTool {}

impl FileReadTool {
    pub fn new() -> Self {
        FileReadTool {}
    }
}

impl Default for FileReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Deserialize)]
pub struct FileReadInput {
    /// 要读取的文件路径
    path: String,
    /// 读取的最大字节数（可选，默认读取全部内容）
    max_bytes: Option<usize>,
    /// 文件大小阈值（字节），超过此大小使用内存映射，默认 10MB
    threshold: Option<usize>,
}

#[derive(Serialize, Deserialize)]
pub struct FileReadOutput {
    /// 文件内容（可能是部分内容）
    content: String,
    /// 文件总大小（字节）
    total_size: u64,
    /// 实际读取的字节数
    bytes_read: usize,
    /// 是否只读取了部分内容
    truncated: bool,
    /// 错误信息（如果有）
    error: Option<String>,
}

impl Describe for FileReadInput {
    fn describe() -> Format {
        vec![
            ("path", "要读取的文件路径").into(),
            ("max_bytes", "读取的最大字节数（可选），默认读取全部内容").into(),
            ("threshold", "文件大小阈值（字节），超过此大小使用内存映射，默认 10MB").into(),
        ]
        .into()
    }
}

impl Describe for FileReadOutput {
    fn describe() -> Format {
        vec![
            ("content", "文件内容").into(),
            ("total_size", "文件总大小（字节）").into(),
            ("bytes_read", "实际读取的字节数").into(),
            ("truncated", "是否只读取了部分内容").into(),
            ("error", "错误信息（如果有）").into(),
        ]
        .into()
    }
}

#[derive(Debug, Error)]
pub enum FileReadError {
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error("文件不存在: {0}")]
    FileNotFound(String),
    #[error("无法读取文件: {0}")]
    ReadError(String),
}

impl ToolError for FileReadError {}

#[async_trait]
impl Tool for FileReadTool {
    type Input = FileReadInput;
    type Output = FileReadOutput;
    type Error = FileReadError;

    async fn invoke_typed(&self, input: &FileReadInput) -> Result<FileReadOutput, FileReadError> {
        let path = PathBuf::from(&input.path);
        
        // 检查文件是否存在
        if !path.exists() {
            return Ok(FileReadOutput {
                content: String::new(),
                total_size: 0,
                bytes_read: 0,
                truncated: false,
                error: Some(format!("文件不存在: {}", input.path)),
            });
        }

        // 获取文件大小
        let metadata = std::fs::metadata(&path)?;
        let file_size = metadata.len() as usize;
        
        // 确定阈值（默认 10MB）
        let threshold = input.threshold.unwrap_or(10 * 1024 * 1024);
        
        // 确定要读取的最大字节数
        let max_bytes = input.max_bytes.unwrap_or(file_size);
        let read_limit = max_bytes.min(file_size);
        
        // 根据文件大小选择读取策略
        let content = if file_size > threshold {
            // 大文件：使用内存映射
            self.read_with_mmap(&path, read_limit)?
        } else {
            // 小文件：使用 read_to_string
            self.read_with_string(&path, read_limit)?
        };

        Ok(FileReadOutput {
            content,
            total_size: file_size as u64,
            bytes_read: read_limit,
            truncated: read_limit < file_size,
            error: None,
        })
    }

    fn description(&self) -> ToolDescription {
        ToolDescription::new(
            "FileReadTool",
            "读取文件内容。小文件使用 read_to_string，大文件使用内存映射（memmap2）以提高效率",
            "使用此工具来查看文件内容。对于大文件，会自动使用内存映射以提高性能",
            FileReadInput::describe(),
            FileReadOutput::describe(),
        )
    }
}

impl FileReadTool {
    /// 小文件：使用 read_to_string
    fn read_with_string(&self, path: &PathBuf, max_bytes: usize) -> Result<String, FileReadError> {
        let mut file = File::open(path)?;
        let mut buffer = vec![0u8; max_bytes];
        let bytes_read = file.read(&mut buffer)?;
        buffer.truncate(bytes_read);
        
        // 尝试转换为字符串，如果失败则返回十六进制表示
        match std::str::from_utf8(&buffer) {
            Ok(s) => Ok(s.to_string()),
            Err(_) => {
                // 二进制文件，返回十六进制预览
                let preview_len = bytes_read.min(512);
                let hex_preview: String = buffer[..preview_len]
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .chunks(16)
                    .map(|chunk| chunk.join(" "))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(format!("[二进制文件，前{}字节的十六进制表示：]\n{}", preview_len, hex_preview))
            }
        }
    }

    /// 大文件：使用内存映射
    fn read_with_mmap(&self, path: &PathBuf, max_bytes: usize) -> Result<String, FileReadError> {
        let file = File::open(path)?;
        let mmap = unsafe { MmapOptions::new().len(max_bytes).map(&file)? };
        
        // 尝试转换为字符串
        match std::str::from_utf8(&mmap[..max_bytes.min(mmap.len())]) {
            Ok(s) => Ok(s.to_string()),
            Err(_) => {
                // 二进制文件，返回十六进制预览
                let preview_len = max_bytes.min(512).min(mmap.len());
                let hex_preview: String = mmap[..preview_len]
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .chunks(16)
                    .map(|chunk| chunk.join(" "))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(format!("[二进制文件（使用内存映射），前{}字节的十六进制表示：]\n{}", preview_len, hex_preview))
            }
        }
    }
}

