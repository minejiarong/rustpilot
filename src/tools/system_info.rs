use async_trait::async_trait;
use llm_chain::tools::{Describe, Format, ToolDescription, Tool, ToolError};
use serde::{Deserialize, Serialize};
use sysinfo::{System, RefreshKind, CpuRefreshKind, ProcessRefreshKind, MemoryRefreshKind, Disks, Networks};
use thiserror::Error;

/// 系统信息查询工具
pub struct SystemInfoTool {}

impl SystemInfoTool {
    pub fn new() -> Self {
        SystemInfoTool {}
    }
}

impl Default for SystemInfoTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Deserialize)]
pub struct SystemInfoInput {
    /// 查询类型：cpu, memory, disk, network, process, all
    query_type: String,
}

#[derive(Serialize, Deserialize)]
pub struct SystemInfoOutput {
    /// 查询结果
    info: String,
    /// 错误信息（如果有）
    error: Option<String>,
}

impl Describe for SystemInfoInput {
    fn describe() -> Format {
        vec![
            ("query_type", "查询类型：cpu（CPU信息）、memory（内存信息）、disk（磁盘信息）、network（网络信息）、process（进程信息）、all（全部信息）").into(),
        ]
        .into()
    }
}

impl Describe for SystemInfoOutput {
    fn describe() -> Format {
        vec![
            ("info", "系统信息文本").into(),
            ("error", "错误信息（如果有）").into(),
        ]
        .into()
    }
}

#[derive(Debug, Error)]
pub enum SystemInfoError {
    #[error(transparent)]
    YamlError(#[from] serde_yaml::Error),
    #[error("系统信息查询失败: {0}")]
    QueryError(String),
}

impl ToolError for SystemInfoError {}

#[async_trait]
impl Tool for SystemInfoTool {
    type Input = SystemInfoInput;
    type Output = SystemInfoOutput;
    type Error = SystemInfoError;

    async fn invoke_typed(&self, input: &SystemInfoInput) -> Result<SystemInfoOutput, SystemInfoError> {
        let query_type = input.query_type.to_lowercase();
        
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything())
                .with_processes(ProcessRefreshKind::everything())
        );

        // 为了获取准确的负载，刷新两次
        sys.refresh_cpu_all();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        sys.refresh_cpu_all();

        let info = match query_type.as_str() {
            "cpu" => self.get_cpu_info(&sys, false),
            "memory" => self.get_memory_info(&sys),
            "disk" => self.get_disk_info(),
            "network" => self.get_network_info(),
            "process" => self.get_process_info(&sys),
            "all" => {
                format!(
                    "{}\n{}\n{}\n{}\n{}",
                    self.get_cpu_info(&sys, true),
                    self.get_memory_info(&sys),
                    self.get_disk_info(),
                    self.get_network_info(),
                    self.get_process_info(&sys)
                )
            }
            _ => {
                return Ok(SystemInfoOutput {
                    info: String::new(),
                    error: Some(format!("未知的查询类型: {}", query_type)),
                });
            }
        };

        Ok(SystemInfoOutput {
            info,
            error: None,
        })
    }

    fn description(&self) -> ToolDescription {
        ToolDescription::new(
            "SystemInfoTool",
            "查询系统实时状态信息，包括CPU负载、内存使用、磁盘空间、网络流量和前5个高消耗进程",
            "使用此工具来获取系统运行状况。支持 cpu, memory, disk, network, process, all 几种查询类型",
            SystemInfoInput::describe(),
            SystemInfoOutput::describe(),
        )
    }
}

impl SystemInfoTool {
    fn get_cpu_info(&self, sys: &System, is_all: bool) -> String {
        let mut output = String::from("--- CPU 信息 ---\n");
        let cpus = sys.cpus();
        output.push_str(&format!("核心数: {}\n", cpus.len()));
        
        // 计算总体负载
        let total_usage: f32 = cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32;
        output.push_str(&format!("总体负载: {:.1}%\n", total_usage));

        if is_all {
            // 如果是 all 模式，只显示负载最高的 2 个核心，避免输出太长
            let mut cpu_list: Vec<_> = cpus.iter().enumerate().collect();
            cpu_list.sort_by(|a, b| b.1.cpu_usage().partial_cmp(&a.1.cpu_usage()).unwrap_or(std::cmp::Ordering::Equal));
            
            output.push_str("核心状态: 仅列出负载最高的2个核心\n");
            for (i, cpu) in cpu_list.iter().take(2) {
                output.push_str(&format!("  Core {}: {:.1}% {}\n", i, cpu.cpu_usage(), cpu.brand()));
            }
        } else {
            // 如果专门查 CPU，显示所有核心
            for (i, cpu) in cpus.iter().enumerate() {
                output.push_str(&format!("  Core {}: {:.1}% {}\n", i, cpu.cpu_usage(), cpu.brand()));
            }
        }
        output
    }

    fn get_memory_info(&self, sys: &System) -> String {
        let mut output = String::from("--- 内存/Swap 信息 ---\n");
        let total_mem = sys.total_memory() / 1024 / 1024;
        let used_mem = sys.used_memory() / 1024 / 1024;
        let total_swap = sys.total_swap() / 1024 / 1024;
        let used_swap = sys.used_swap() / 1024 / 1024;
        
        let mem_perc = if total_mem > 0 { (used_mem as f64 / total_mem as f64) * 100.0 } else { 0.0 };
        output.push_str(&format!("内存: {}MB / {}MB ({:.1}%)\n", used_mem, total_mem, mem_perc));
        output.push_str(&format!("Swap: {}MB / {}MB\n", used_swap, total_swap));
        output
    }

    fn get_disk_info(&self) -> String {
        let mut output = String::from("--- 磁盘信息 ---\n");
        let disks = Disks::new_with_refreshed_list();
        for disk in &disks {
            let total = disk.total_space() / 1024 / 1024 / 1024;
            let available = disk.available_space() / 1024 / 1024 / 1024;
            output.push_str(&format!(
                "盘符: {:?} | 类型: {:?} | 文件系统: {} | 剩余: {}GB / {}GB\n",
                disk.mount_point(),
                disk.kind(),
                disk.file_system().to_string_lossy(),
                available,
                total
            ));
        }
        output
    }

    fn get_network_info(&self) -> String {
        let mut output = String::from("--- 网络接口 ---\n");
        let networks = Networks::new_with_refreshed_list();
        for (interface_name, data) in &networks {
            output.push_str(&format!(
                "接口: {} | 接收: {} B | 发送: {} B\n",
                interface_name,
                data.received(),
                data.transmitted()
            ));
        }
        output
    }

    fn get_process_info(&self, sys: &System) -> String {
        let mut output = String::from("--- 进程概览 (Top 5 CPU使用) ---\n");
        let mut processes: Vec<_> = sys.processes().values().collect();
        
        // 按CPU使用率降序排序
        processes.sort_by(|a, b| b.cpu_usage().partial_cmp(&a.cpu_usage()).unwrap_or(std::cmp::Ordering::Equal));
        
        for p in processes.iter().take(5) {
            output.push_str(&format!(
                "PID: {:<6} | CPU: {:>5.1}% | MEM: {:>7} KB | 名称: {}\n",
                p.pid(),
                p.cpu_usage(),
                p.memory() / 1024,
                p.name().to_string_lossy()
            ));
        }
        output
    }
}
