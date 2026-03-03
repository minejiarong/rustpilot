use async_trait::async_trait;
use llm_chain::tools::{Describe, Format, ToolDescription, Tool, ToolError};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tar::{Archive, Builder};

/// Zstd 压缩与解压缩工具
pub struct ZstdTool {}

impl ZstdTool {
    pub fn new() -> Self {
        ZstdTool {}
    }
}

impl Default for ZstdTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Deserialize)]
pub struct ZstdInput {
    /// 操作类型：compress (压缩) 或 decompress (解压缩)
    operation: String,
    /// 目标文件或文件夹路径
    path: String,
    /// 压缩等级 (1-21)，默认为 3
    level: Option<i32>,
}

#[derive(Serialize, Deserialize)]
pub struct ZstdOutput {
    /// 操作结果描述
    message: String,
    /// 生成的文件或目录路径
    output_path: Option<String>,
    /// 错误信息
    error: Option<String>,
}

impl Describe for ZstdInput {
    fn describe() -> Format {
        vec![
            ("operation", "操作类型：'compress' 或 'decompress'").into(),
            ("path", "目标路径").into(),
            ("level", "压缩等级 (1-21)，可选，默认为 3").into(),
        ]
        .into()
    }
}

impl Describe for ZstdOutput {
    fn describe() -> Format {
        vec![
            ("message", "执行结果描述").into(),
            ("output_path", "生成的文件或目录路径").into(),
            ("error", "错误信息").into(),
        ]
        .into()
    }
}

#[derive(Debug, Error)]
pub enum ZstdToolError {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
    #[error("不支持的操作类型: {0}")]
    UnsupportedOperation(String),
    #[error("路径不存在: {0}")]
    PathNotFound(String),
    #[error("解压失败: {0}")]
    DecompressError(String),
}

impl ToolError for ZstdToolError {}

#[async_trait]
impl Tool for ZstdTool {
    type Input = ZstdInput;
    type Output = ZstdOutput;
    type Error = ZstdToolError;

    async fn invoke_typed(&self, input: &ZstdInput) -> Result<ZstdOutput, ZstdToolError> {
        let path = PathBuf::from(&input.path);
        
        if !path.exists() {
            return Ok(ZstdOutput {
                message: "操作失败".to_string(),
                output_path: None,
                error: Some(format!("路径不存在: {}", input.path)),
            });
        }

        match input.operation.as_str() {
            "compress" => self.compress(&path, input.level.unwrap_or(3)).await,
            "decompress" => self.decompress(&path).await,
            _ => Err(ZstdToolError::UnsupportedOperation(input.operation.clone())),
        }
    }

    fn description(&self) -> ToolDescription {
        ToolDescription::new(
            "ZstdTool",
            "使用 Zstd 算法进行压缩和解压缩。支持单个文件和整个文件夹（文件夹会自动先打包为 tar）。",
            "操作 compress 会在同级目录生成 .zst 或 .tar.zst 文件；decompress 会在同级目录创建同名文件夹并解压。",
            ZstdInput::describe(),
            ZstdOutput::describe(),
        )
    }
}

impl ZstdTool {
    async fn compress(&self, path: &Path, level: i32) -> Result<ZstdOutput, ZstdToolError> {
        let is_dir = path.is_dir();
        let output_path = if is_dir {
            path.with_extension("tar.zst")
        } else {
            path.with_extension(format!("{}.zst", path.extension().and_then(|e| e.to_str()).unwrap_or("file")))
        };

        let mut output_file = File::create(&output_path)?;
        let mut encoder = zstd::stream::Encoder::new(output_file, level)?;

        if is_dir {
            let mut tar_builder = Builder::new(encoder);
            tar_builder.append_dir_all(".", path)?;
            let mut encoder = tar_builder.into_inner()?;
            encoder.finish()?;
        } else {
            let mut input_file = File::open(path)?;
            io::copy(&mut input_file, &mut encoder)?;
            encoder.finish()?;
        }

        Ok(ZstdOutput {
            message: format!("压缩完成: {}", if is_dir { "文件夹已打包并压缩" } else { "文件已压缩" }),
            output_path: Some(output_path.to_string_lossy().to_string()),
            error: None,
        })
    }

    async fn decompress(&self, path: &Path) -> Result<ZstdOutput, ZstdToolError> {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("extracted");
        
        // 移除 .zst 后缀作为文件夹名
        let folder_name = if file_name.ends_with(".tar.zst") {
            &file_name[..file_name.len() - 8]
        } else if file_name.ends_with(".zst") {
            &file_name[..file_name.len() - 4]
        } else {
            file_name
        };

        let mut extract_to = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        extract_to.push(folder_name);

        if !extract_to.exists() {
            fs::create_dir_all(&extract_to)?;
        }

        let input_file = File::open(path)?;
        let decoder = zstd::stream::Decoder::new(input_file)?;

        if file_name.ends_with(".tar.zst") {
            let mut archive = Archive::new(decoder);
            archive.unpack(&extract_to)?;
        } else {
            // 单个文件解压到该文件夹下，保持原文件名（去掉 .zst）
            let mut output_file_path = extract_to.clone();
            output_file_path.push(folder_name);
            let mut output_file = File::create(output_file_path)?;
            let mut decoder = decoder;
            io::copy(&mut decoder, &mut output_file)?;
        }

        Ok(ZstdOutput {
            message: "解压缩完成".to_string(),
            output_path: Some(extract_to.to_string_lossy().to_string()),
            error: None,
        })
    }
}

