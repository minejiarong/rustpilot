use async_trait::async_trait;
use llm_chain::tools::{Describe, Format, ToolDescription, Tool, ToolError};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::string::FromUtf8Error;
use thiserror::Error;

/// 安全命令执行工具，避免执行危险命令
pub struct SafeCommandTool {}

impl SafeCommandTool {
    pub fn new() -> Self {
        SafeCommandTool {}
    }
}

impl Default for SafeCommandTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Deserialize)]
pub struct SafeCommandInput {
    /// 要执行的命令（仅限安全命令）
    command: String,
    /// 命令参数
    args: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
pub struct SafeCommandOutput {
    /// 标准输出
    stdout: String,
    /// 标准错误输出
    stderr: String,
    /// 退出状态码（0表示成功）
    status: i32,
    /// 错误信息（如果有）
    error: Option<String>,
}

impl Describe for SafeCommandInput {
    fn describe() -> Format {
        vec![
            ("command", "要执行的命令名称（仅限安全命令，如 ls, dir, echo, cat, type等）").into(),
            ("args", "命令参数列表").into(),
        ]
        .into()
    }
}

impl Describe for SafeCommandOutput {
    fn describe() -> Format {
        vec![
            ("stdout", "命令的标准输出").into(),
            ("stderr", "命令的标准错误输出").into(),
            ("status", "退出状态码，0表示成功").into(),
            ("error", "错误信息（如果有）").into(),
        ]
        .into()
    }
}

#[derive(Debug, Error)]
pub enum SafeCommandError {
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    FromUtf8Error(#[from] FromUtf8Error),
    #[error("不允许执行危险命令: {0}")]
    DangerousCommand(String),
}

impl ToolError for SafeCommandError {}

// 危险命令黑名单
const DANGEROUS_COMMANDS: &[&str] = &[
    "rm", "rmdir", "del", "erase", "format", "mkfs", "dd",
    "shutdown", "reboot", "halt", "poweroff",
    "sudo", "su", "chmod", "chown",
    "kill", "killall", "pkill", "taskkill",
    "mv", "move", "rename",
    "fdisk", "parted", "gparted",
    "curl", "wget", "nc", "netcat",
    "python", "python3", "node", "ruby", "perl", "bash", "sh", "cmd", "powershell",
];

impl SafeCommandTool {
    fn is_dangerous(&self, command: &str) -> bool {
        let cmd_lower = command.to_lowercase();
        DANGEROUS_COMMANDS.iter().any(|&dangerous| {
            cmd_lower == dangerous || cmd_lower.starts_with(&format!("{} ", dangerous))
        })
    }
}

#[async_trait]
impl Tool for SafeCommandTool {
    type Input = SafeCommandInput;
    type Output = SafeCommandOutput;
    type Error = SafeCommandError;

    async fn invoke_typed(&self, input: &SafeCommandInput) -> Result<SafeCommandOutput, SafeCommandError> {
        // 检查是否为危险命令
        if self.is_dangerous(&input.command) {
            return Ok(SafeCommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                status: -1,
                error: Some(format!("不允许执行危险命令: {}", input.command)),
            });
        }

        // 执行命令
        let mut cmd = Command::new(&input.command);
        
        if let Some(ref args) = input.args {
            cmd.args(args);
        }

        let output = cmd.output()?;

        Ok(SafeCommandOutput {
            stdout: String::from_utf8(output.stdout)?,
            stderr: String::from_utf8(output.stderr)?,
            status: output.status.code().unwrap_or(-1),
            error: None,
        })
    }

    fn description(&self) -> ToolDescription {
        ToolDescription::new(
            "SafeCommandTool",
            "执行安全的系统命令，禁止执行删除、格式化、系统控制等危险操作",
            "使用此工具来执行只读或安全的系统命令，如列出文件、查看文件内容等",
            SafeCommandInput::describe(),
            SafeCommandOutput::describe(),
        )
    }
}
