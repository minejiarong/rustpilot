use async_trait::async_trait;
use llm_chain::tools::{Describe, Format, ToolDescription, Tool, ToolError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// 文件系统操作工具，支持改名、删除、拷贝、移动等操作
pub struct FileSystemTool {}

impl FileSystemTool {
    pub fn new() -> Self {
        FileSystemTool {}
    }
}

impl Default for FileSystemTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Deserialize)]
pub struct FileSystemInput {
    /// 操作类型：create（创建目录）、rename（改名/移动）、delete（删除）、copy（拷贝）、move（移动）
    operation: String,
    /// 源路径（文件或目录），单个操作时使用
    source: Option<String>,
    /// 源路径列表（批量操作时使用），如果提供则优先使用
    sources: Option<Vec<String>>,
    /// 目标路径（rename/copy/move 时需要）
    destination: Option<String>,
    /// 是否递归操作目录（delete 时有效）
    recursive: Option<bool>,
}

#[derive(Serialize, Deserialize)]
pub struct FileSystemOutput {
    /// 操作结果描述
    message: String,
    /// 操作的文件/目录数量
    items_processed: usize,
    /// 目标路径（如果有）
    destination: Option<String>,
    /// 错误信息
    error: Option<String>,
}

impl Describe for FileSystemInput {
    fn describe() -> Format {
        vec![
            ("operation", "操作类型：'create'（创建目录）、'rename'（改名/移动）、'delete'（删除）、'copy'（拷贝）、'move'（移动）").into(),
            ("source", "源路径（单个操作时使用），create 操作时为目标目录路径").into(),
            ("sources", "源路径列表（批量操作时使用，优先级高于 source），例如 ['file1.txt', 'file2.txt']").into(),
            ("destination", "目标路径（rename/copy/move 时需要）。批量操作时，所有文件会移动到/复制到该目录下，保持原文件名").into(),
            ("recursive", "是否递归删除目录（仅 delete 操作有效，默认 false）").into(),
        ]
        .into()
    }
}

impl Describe for FileSystemOutput {
    fn describe() -> Format {
        vec![
            ("message", "操作结果描述").into(),
            ("items_processed", "操作的文件/目录数量").into(),
            ("destination", "目标路径（如果有）").into(),
            ("error", "错误信息").into(),
        ]
        .into()
    }
}

#[derive(Debug, Error)]
pub enum FileSystemError {
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error("不支持的操作类型: {0}")]
    UnsupportedOperation(String),
    #[error("缺少必要参数: {0}")]
    MissingParameter(String),
    #[error("路径不存在: {0}")]
    PathNotFound(String),
}

impl ToolError for FileSystemError {}

#[async_trait]
impl Tool for FileSystemTool {
    type Input = FileSystemInput;
    type Output = FileSystemOutput;
    type Error = FileSystemError;

    async fn invoke_typed(&self, input: &FileSystemInput) -> Result<FileSystemOutput, FileSystemError> {
        // 判断是批量操作还是单个操作
        let sources = if let Some(ref sources_list) = input.sources {
            sources_list.clone()
        } else if let Some(ref source) = input.source {
            vec![source.clone()]
        } else {
            return Err(FileSystemError::MissingParameter("source 或 sources 参数是必需的".to_string()));
        };

        // 批量操作处理
        if sources.len() > 1 {
            return self.handle_batch_operation(input, &sources).await;
        }

        // 单个操作处理
        let source = &sources[0];
        let source_path = PathBuf::from(source);
        
        match input.operation.to_lowercase().as_str() {
            "create" | "mkdir" => {
                // 创建目录（如果已存在则忽略错误）
                if source_path.exists() {
                    return Ok(FileSystemOutput {
                        message: format!("目录已存在: {}", source),
                        items_processed: 0,
                        destination: Some(source.clone()),
                        error: None,
                    });
                }
                
                fs::create_dir_all(&source_path)?;
                
                Ok(FileSystemOutput {
                    message: format!("成功创建目录: {}", source),
                    items_processed: 1,
                    destination: Some(source.clone()),
                    error: None,
                })
            }
            "rename" | "move" => {
                if !source_path.exists() {
                    return Ok(FileSystemOutput {
                        message: "操作失败".to_string(),
                        items_processed: 0,
                        destination: None,
                        error: Some(format!("源路径不存在: {}", source)),
                    });
                }
                let dest = input.destination.as_ref()
                    .ok_or_else(|| FileSystemError::MissingParameter("destination 参数是必需的".to_string()))?;
                let dest_path = PathBuf::from(dest);
                
                // 确保目标目录存在
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                
                fs::rename(&source_path, &dest_path)?;
                
                Ok(FileSystemOutput {
                    message: format!("成功{}: {} -> {}", 
                        if input.operation == "rename" { "改名" } else { "移动" },
                        source, dest),
                    items_processed: 1,
                    destination: Some(dest.clone()),
                    error: None,
                })
            }
            "copy" => {
                if !source_path.exists() {
                    return Ok(FileSystemOutput {
                        message: "操作失败".to_string(),
                        items_processed: 0,
                        destination: None,
                        error: Some(format!("源路径不存在: {}", source)),
                    });
                }
                
                let dest = input.destination.as_ref()
                    .ok_or_else(|| FileSystemError::MissingParameter("destination 参数是必需的".to_string()))?;
                let dest_path = PathBuf::from(dest);
                
                // 确保目标目录存在
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                
                let items_count = if source_path.is_dir() {
                    self.copy_dir(&source_path, &dest_path)?
                } else {
                    fs::copy(&source_path, &dest_path)?;
                    1
                };
                
                Ok(FileSystemOutput {
                    message: format!("成功拷贝: {} -> {}", source, dest),
                    items_processed: items_count,
                    destination: Some(dest.clone()),
                    error: None,
                })
            }
            "delete" => {
                if !source_path.exists() {
                    return Ok(FileSystemOutput {
                        message: "操作失败".to_string(),
                        items_processed: 0,
                        destination: None,
                        error: Some(format!("源路径不存在: {}", source)),
                    });
                }
                
                let recursive = input.recursive.unwrap_or(false);
                let items_count = if source_path.is_dir() {
                    if recursive {
                        self.remove_dir_recursive(&source_path)?
                    } else {
                        fs::remove_dir(&source_path)?;
                        1
                    }
                } else {
                    fs::remove_file(&source_path)?;
                    1
                };
                
                Ok(FileSystemOutput {
                    message: format!("成功删除: {}", source),
                    items_processed: items_count,
                    destination: None,
                    error: None,
                })
            }
            _ => Err(FileSystemError::UnsupportedOperation(input.operation.clone())),
        }
    }

    fn description(&self) -> ToolDescription {
        ToolDescription::new(
            "FileSystemTool",
            "文件系统操作工具，支持创建目录、改名、删除、拷贝、移动文件和目录。支持批量操作（使用 sources 参数）",
            "使用此工具进行文件操作。路径请使用正斜杠 '/'。批量操作时，提供 sources 列表和 destination 目录，所有文件会被移动/复制到目标目录下。create 操作会自动创建所有父目录。copy 操作会自动递归处理目录。",
            FileSystemInput::describe(),
            FileSystemOutput::describe(),
        )
    }
}

impl FileSystemTool {
    /// 递归拷贝目录
    fn copy_dir(&self, src: &Path, dst: &Path) -> Result<usize, FileSystemError> {
        fs::create_dir_all(dst)?;
        let mut count = 1; // 目录本身
        
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            
            if src_path.is_dir() {
                count += self.copy_dir(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)?;
                count += 1;
            }
        }
        
        Ok(count)
    }
    
    /// 递归删除目录
    fn remove_dir_recursive(&self, path: &Path) -> Result<usize, FileSystemError> {
        let mut count = 0;
        
        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let entry_path = entry.path();
                
                if entry_path.is_dir() {
                    count += self.remove_dir_recursive(&entry_path)?;
                } else {
                    fs::remove_file(&entry_path)?;
                    count += 1;
                }
            }
            fs::remove_dir(path)?;
            count += 1; // 目录本身
        }
        
        Ok(count)
    }

    /// 处理批量操作
    async fn handle_batch_operation(&self, input: &FileSystemInput, sources: &[String]) -> Result<FileSystemOutput, FileSystemError> {
        let operation = input.operation.to_lowercase();
        let mut success_count = 0;
        let mut failed_paths = Vec::new();

        match operation.as_str() {
            "move" | "copy" => {
                let dest_dir = input.destination.as_ref()
                    .ok_or_else(|| FileSystemError::MissingParameter("批量操作需要 destination 参数（目标目录）".to_string()))?;
                let dest_path = PathBuf::from(dest_dir);
                
                // 确保目标目录存在
                fs::create_dir_all(&dest_path)?;

                for source in sources {
                    let source_path = PathBuf::from(source);
                    if !source_path.exists() {
                        failed_paths.push(format!("{} (不存在)", source));
                        continue;
                    }

                    let file_name = source_path.file_name()
                        .ok_or_else(|| FileSystemError::MissingParameter(format!("无法获取文件名: {}", source)))?;
                    let target_path = dest_path.join(file_name);

                    match operation.as_str() {
                        "move" => {
                            if let Err(e) = fs::rename(&source_path, &target_path) {
                                failed_paths.push(format!("{} ({})", source, e));
                            } else {
                                success_count += 1;
                            }
                        }
                        "copy" => {
                            if source_path.is_dir() {
                                match self.copy_dir(&source_path, &target_path) {
                                    Ok(_) => success_count += 1,
                                    Err(e) => failed_paths.push(format!("{} ({})", source, e)),
                                }
                            } else {
                                if let Err(e) = fs::copy(&source_path, &target_path) {
                                    failed_paths.push(format!("{} ({})", source, e));
                                } else {
                                    success_count += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                }

                let message = if failed_paths.is_empty() {
                    format!("成功{} {} 个文件到 {}", 
                        if operation == "move" { "移动" } else { "拷贝" },
                        success_count, dest_dir)
                } else {
                    format!("成功{} {} 个文件，失败 {} 个: {}", 
                        if operation == "move" { "移动" } else { "拷贝" },
                        success_count, failed_paths.len(), failed_paths.join(", "))
                };

                Ok(FileSystemOutput {
                    message,
                    items_processed: success_count,
                    destination: Some(dest_dir.clone()),
                    error: if failed_paths.is_empty() { None } else { Some(format!("{} 个文件操作失败", failed_paths.len())) },
                })
            }
            "delete" => {
                let recursive = input.recursive.unwrap_or(false);
                
                for source in sources {
                    let source_path = PathBuf::from(source);
                    if !source_path.exists() {
                        failed_paths.push(format!("{} (不存在)", source));
                        continue;
                    }

                    let result = if source_path.is_dir() {
                        if recursive {
                            self.remove_dir_recursive(&source_path)
                        } else {
                            fs::remove_dir(&source_path).map(|_| 1).map_err(|e| e.into())
                        }
                    } else {
                        fs::remove_file(&source_path).map(|_| 1).map_err(|e| e.into())
                    };

                    match result {
                        Ok(count) => success_count += count,
                        Err(e) => failed_paths.push(format!("{} ({})", source, e)),
                    }
                }

                let message = if failed_paths.is_empty() {
                    format!("成功删除 {} 个文件/目录", success_count)
                } else {
                    format!("成功删除 {} 个文件/目录，失败 {} 个: {}", 
                        success_count, failed_paths.len(), failed_paths.join(", "))
                };

                Ok(FileSystemOutput {
                    message,
                    items_processed: success_count,
                    destination: None,
                    error: if failed_paths.is_empty() { None } else { Some(format!("{} 个文件/目录删除失败", failed_paths.len())) },
                })
            }
            _ => Err(FileSystemError::UnsupportedOperation(format!("批量操作不支持: {}", operation))),
        }
    }
}

