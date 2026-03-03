mod tools;
mod config;

use clap::{Parser, Subcommand};
use llm_chain::multitool;
use llm_chain::tools::{ToolCollection, ToolDescription, Tool, ToolError};
use llm_chain::{chains::conversation::Chain, executor, options, parameters, prompt, step::Step};
use std::io::{self, Write};
use serde::{Serialize, Deserialize};
use thiserror::Error;
use async_trait::async_trait;
use chrono::Local;
use owo_colors::OwoColorize;
use tabled::{Table, Tabled, settings::Style};
use tools::*;

// 使用multitool宏创建工具集合
multitool!(
    RustPilotToolbox,
    RustPilotToolboxInput,
    RustPilotToolboxOutput,
    RustPilotToolboxError,
    FileSearchTool,
    FileSearchInput,
    FileSearchOutput,
    FileSearchError,
    FileReadTool,
    FileReadInput,
    FileReadOutput,
    FileReadError,
    FileSystemTool,
    FileSystemInput,
    FileSystemOutput,
    FileSystemError,
    WeatherTool,
    WeatherInput,
    WeatherOutput,
    WeatherError,
    SystemInfoTool,
    SystemInfoInput,
    SystemInfoOutput,
    SystemInfoError,
    SafeCommandTool,
    SafeCommandInput,
    SafeCommandOutput,
    SafeCommandError,
    RssTool,
    RssInput,
    RssOutput,
    RssError,
    ZstdTool,
    ZstdInput,
    ZstdOutput,
    ZstdToolError
);

#[derive(Parser)]
#[command(name = "rustpilot")]
#[command(about = "基于Rust的本地命令行智能助手", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    
    /// 交互式模式（默认）
    #[arg(short, long)]
    interactive: bool,
    
    /// 单次查询模式
    #[arg(short, long)]
    query: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// 配置设置
    Config {
        /// LLM提供者：ollama, openai
        #[arg(short, long)]
        provider: Option<String>,
        /// API基础URL
        #[arg(short, long)]
        api_base: Option<String>,
        /// API密钥
        #[arg(short, long)]
        api_key: Option<String>,
        /// 模型名称
        #[arg(short, long)]
        model: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    
    let cli = Cli::parse();
    
    // 处理配置命令
    if let Some(Commands::Config { provider, api_base, api_key, model }) = cli.command {
        println!("{}", "配置功能待实现".yellow());
        return Ok(());
    }
    
    // 加载配置
    let config = config::Config::from_env();
    
    // 创建工具集合
    let mut tool_collection = ToolCollection::<RustPilotToolbox>::new();
    tool_collection.add_tool(FileSearchTool::new().into());
    tool_collection.add_tool(FileReadTool::new().into());
    tool_collection.add_tool(FileSystemTool::new().into());
    tool_collection.add_tool(WeatherTool::new(config.weather_api_key.clone()).into());
    tool_collection.add_tool(SystemInfoTool::new().into());
    tool_collection.add_tool(SafeCommandTool::new().into());
    tool_collection.add_tool(RssTool::new().into());
    tool_collection.add_tool(ZstdTool::new().into());
    
    // 创建工具提示模板
    let tool_prompt = tool_collection.to_prompt_template()?;
    
    // 获取当前系统时间
    let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let time_prompt = format!("当前系统时间：{}\n", current_time);
    
    // 获取当前工作目录
    let current_dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .to_string_lossy()
        .to_string();
    let dir_prompt = format!("当前工作目录：{}\n", current_dir);
    
    // 创建对话链
    let system_prompt = prompt::StringTemplate::combine(vec![
        prompt::StringTemplate::static_string(time_prompt),
        prompt::StringTemplate::static_string(dir_prompt),
        tool_prompt,
        prompt::StringTemplate::static_string(
            "你是一个实战型智能助手，你的目标是亲自为用户解决问题，而不是教用户怎么写代码。\
            \
            重要禁令（违反将导致任务失败）：\
            1. 严禁提供任何代码（如 Python、Shell、PowerShell 等）让用户手动运行。凡是涉及文件操作、搜索、系统查询等任务，必须且只能使用提供的工具来完成。\
            2. 严禁输出任何 Markdown 代码块（如 ```python、```rust 等），唯一的例外是用于工具调用的 ```yaml 块。\
            3. 严禁臆造工具，必须使用下方白名单中的工具。\
            \
            可用工具白名单：\
            - FileSearchTool: 搜索文件。\
            - FileReadTool: 读取文件。\
            - FileSystemTool: 创建目录、改名、移动、复制、删除。\
            - WeatherTool: 天气查询。\
            - SystemInfoTool: 系统监控。\
            - SafeCommandTool: 执行终端命令。\
            - RssTool: 新闻阅读。\
            - ZstdTool: 压缩解压。\
            \
            响应准则：\
            1. 任务导向：如果用户要求移动文件、搜索内容等，立即调用工具。不要解释怎么做，直接做。\
            2. 格式规范：工具调用必须严格遵守 YAML 格式，严禁 JSON。\
            3. 多步执行：你可以连续调用工具。例如：先搜索，根据结果再创建目录，最后移动文件。不要分多次对话，直接连续出招。\
            4. 确认机制：涉及删除或大批量修改，先描述计划并问“是否继续？”，得到确认后再执行下一步。\
            5. 输出限制：使用纯文本，严禁 Markdown（粗体、标题等）。路径统一使用正斜杠 '/'。"
        ),
    ]);
    
    // 根据配置创建executor并运行
    if config.is_ollama() || config.is_openai() || config.is_deepseek() || config.is_remote() {
        run_with_executor::<llm_chain_openai::chatgpt::Executor>(
            &config, tool_collection, system_prompt, &cli
        ).await?;
    } else {
        return Err("不支持的LLM提供者".into());
    }
    
    Ok(())
}

async fn run_with_executor<E: llm_chain::traits::Executor>(
    config: &config::Config,
    tool_collection: ToolCollection<RustPilotToolbox>,
    system_prompt: prompt::StringTemplate,
    cli: &Cli,
) -> Result<(), Box<dyn std::error::Error>> {
    // 设置API基础URL环境变量
    let api_base_url = if config.is_ollama() {
        // 如果是Ollama，使用默认的Ollama地址或配置的地址
        Some(config.api_base_url.clone().unwrap_or("http://localhost:11434".to_string()))
    } else {
        // 为DeepSeek模型设置默认的API基础URL
        if let Some(ref url) = config.api_base_url {
            Some(url.clone())
        } else if config.model.starts_with("deepseek-") {
            // DeepSeek模型的默认API基础URL（包含/v1后缀，符合async-openai库的预期）
            Some("https://api.deepseek.com/v1".to_string())
        } else {
            // 其他模型使用配置的地址
            config.api_base_url.clone()
        }
    };
    
    if let Some(ref url) = api_base_url {
        // 清理URL，移除常见的后缀，因为async-openai会自动添加它们
        let cleaned_url = url
            .strip_suffix("/chat/completions")
            .or_else(|| url.strip_suffix("/chat/completions/"))
            .or_else(|| url.strip_suffix("/completions"))
            .or_else(|| url.strip_suffix("/completions/"))
            .unwrap_or(url)
            .trim_end_matches('/')
            .to_string();
            
        std::env::set_var("OPENAI_API_BASE_URL", &cleaned_url);
    }
    
    // 创建executor
    let mut opts_builder = options::Options::builder();
    if let Some(ref api_key) = config.api_key {
        opts_builder.add_option(options::Opt::ApiKey(api_key.clone()));
    }
    
    // 如果是通过Ollama转接远程DeepSeek API，需要确保模型名称正确
    // Ollama支持使用格式：model:tag@api_base_url来指定远程模型
    let actual_model = if config.is_ollama() && config.model.starts_with("deepseek-") {
        // 如果模型名称以deepseek-开头，且使用Ollama提供者
        // 确保模型名称格式正确
        config.model.clone()
    } else {
        config.model.clone()
    };
    
    // 确定用于token计数的模型名称
    let _model_for_token_count = match config.provider.as_str() {
        "ollama" | "deepseek" | "remote" => {
            // 对于Ollama和DeepSeek模型，使用gpt-3.5-turbo的tokenizer进行计数
            "gpt-3.5-turbo"
        },
        _ => {
            // 其他情况使用原始模型名称
            &config.model
        }
    };
    
    // 添加模型选项：使用实际模型名称进行API调用
    opts_builder.add_option(options::Opt::Model(options::ModelRef::from_model_name(&actual_model)));
    let opts = opts_builder.build();
    
    let exec = executor!(chatgpt, opts)?;

    let mut chain = Chain::new(llm_chain::prompt::Data::Text(system_prompt))?;
    
    // 单次查询模式
    if let Some(ref query) = cli.query {
        process_query::<llm_chain_openai::chatgpt::Executor>(&mut chain, &exec, &tool_collection, query).await?;
        return Ok(());
    }
    
    // 交互式模式
    println!("\n{}", "RustPilot 智能助手已启动！".green().bold());
    println!("{}", "输入 'exit' 或 'quit' 退出，输入 'help' 查看帮助\n".bright_black());
    
    loop {
        print!("{}", "> ".bright_blue().bold());
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();
        
        if input.is_empty() {
            continue;
        }
        
        if input == "exit" || input == "quit" {
            println!("{}", "再见！".green());
            break;
        }
        
        if input == "help" {
            print_help();
            continue;
        }
        
        if let Err(e) = process_query::<llm_chain_openai::chatgpt::Executor>(&mut chain, &exec, &tool_collection, input).await {
            eprintln!("{} {}", "错误:".red().bold(), e);
        }
    }
    
    Ok(())
}

// 清理输出文本，移除助手角色前缀
fn clean_output(text: &str) -> String {
    let mut s = text.trim();
    let prefixes = ["Assistant:", "Assistant", "助手:", "助手"];
    
    loop {
        let mut changed = false;
        for p in prefixes {
            if s.starts_with(p) {
                s = s[p.len()..].trim();
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    s.to_string()
}

// 检测用户输入是否为确认词
fn is_confirmation(input: &str) -> bool {
    let lower = input.to_lowercase();
    let trimmed = lower.trim();
    matches!(trimmed, "yes" | "y" | "继续" | "好的" | "是" | "确认" | "ok" | "okay" | "可以" | "行" | "嗯")
}

#[derive(Tabled)]
struct KeyValue {
    #[tabled(rename = "属性")]
    key: String,
    #[tabled(rename = "内容")]
    value: String,
}

#[derive(Tabled)]
struct FileRow {
    #[tabled(rename = "文件名")]
    name: String,
    #[tabled(rename = "大小")]
    size: String,
    #[tabled(rename = "修改时间")]
    modified: String,
}

/// 将工具输出的 Key-Value 文本转换为表格，支持列表检测
fn format_tool_output_as_table(text: &str) -> String {
    // 首先尝试清理 YAML 序列化带来的外壳（如 info: "..." 或 info: |）
    let mut clean_lines = Vec::new();
    let mut is_in_info_block = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("info:") {
            is_in_info_block = true;
            let content = trimmed.strip_prefix("info:").unwrap().trim();
            // 如果是一行内容（带引号），提取引号内的内容
            if (content.starts_with('"') && content.ends_with('"')) || (content.starts_with('\'') && content.ends_with('\'')) {
                let unescaped = content[1..content.len()-1].replace("\\n", "\n").replace("\\\"", "\"");
                for l in unescaped.lines() { clean_lines.push(l.to_string()); }
                is_in_info_block = false;
            } else if content == "|" || content == ">" {
                // 多行块引导符，继续处理后续行
                continue;
            } else if !content.is_empty() {
                clean_lines.push(content.to_string());
            }
        } else if is_in_info_block {
            if trimmed.starts_with("error:") || trimmed.starts_with("count:") {
                is_in_info_block = false;
            } else {
                // 移除多行块可能的缩进
                clean_lines.push(line.trim_start().to_string());
            }
        } else if !trimmed.is_empty() && !trimmed.starts_with("error: null") {
            clean_lines.push(line.to_string());
        }
    }

    if clean_lines.is_empty() { return text.to_string(); }
    let processed_text = clean_lines.join("\n");

    // 1. 优先检测是否为文件列表 (FileSearchTool)
    if processed_text.contains("path:") && processed_text.contains("size:") {
        let mut summary_info = Vec::new();
        let mut file_rows = Vec::new();
        let mut current_name = String::new();
        let mut current_size = String::new();
        let mut current_modified = String::new();

        for line in &clean_lines {
            let trimmed = line.trim();
            if trimmed.starts_with("- path:") || trimmed.starts_with("path:") {
                if !current_name.is_empty() {
                    file_rows.push(FileRow { name: current_name.clone(), size: current_size.clone(), modified: current_modified.clone() });
                }
                let path = trimmed.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
                current_name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
            } else if trimmed.starts_with("size:") {
                let bytes = trimmed.split_once(':').map(|(_, v)| v.trim().parse::<u64>().unwrap_or(0)).unwrap_or(0);
                current_size = if bytes > 1024 * 1024 { format!("{:.2} MB", bytes as f64 / 1048576.0) } else { format!("{:.2} KB", bytes as f64 / 1024.0) };
            } else if trimmed.starts_with("modified:") {
                current_modified = trimmed.split_once(':').map(|(_, v)| v.trim().to_string()).unwrap_or_default();
            } else if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim();
                if matches!(key, "count" | "search_duration_ms" | "is_truncated") {
                    summary_info.push(KeyValue { 
                        key: match key { "count" => "找到文件数", "search_duration_ms" => "搜索耗时", "is_truncated" => "是否截断", _ => key }.to_string(),
                        value: if key == "search_duration_ms" { format!("{} ms", v.trim()) } else { v.trim().to_string() }
                    });
                }
            }
        }
        if !current_name.is_empty() {
            file_rows.push(FileRow { name: current_name, size: current_size, modified: current_modified });
        }

        let mut output = String::new();
        if !summary_info.is_empty() { output.push_str(&Table::new(summary_info).with(Style::modern()).to_string()); output.push('\n'); }
        if !file_rows.is_empty() { output.push_str(&Table::new(file_rows).with(Style::modern()).to_string()); }
        return output;
    }

    // 2. 检测是否包含多段式系统信息 (SystemInfoTool)
    if processed_text.contains("---") {
        let mut final_output = String::new();
        let mut current_section = String::new();
        let mut section_rows = Vec::new();

        for line in &clean_lines {
            let trimmed = line.trim();
            if trimmed.starts_with("---") && trimmed.ends_with("---") {
                if !section_rows.is_empty() {
                    final_output.push_str(&format!("\n【 {} 】\n", current_section.bold().yellow()));
                    final_output.push_str(&Table::new(section_rows).with(Style::modern()).to_string());
                    final_output.push('\n');
                }
                current_section = trimmed.trim_matches('-').trim().to_string();
                section_rows = Vec::new();
            } else if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim().trim_start_matches("- ").to_string();
                let value = v.trim().to_string();
                if !key.is_empty() && !value.is_empty() {
                    section_rows.push(KeyValue { key, value });
                }
            } else if !trimmed.is_empty() && !current_section.is_empty() {
                section_rows.push(KeyValue { key: "·".to_string(), value: trimmed.to_string() });
            }
        }
        if !section_rows.is_empty() {
            final_output.push_str(&format!("\n【 {} 】\n", current_section.bold().yellow()));
            final_output.push_str(&Table::new(section_rows).with(Style::modern()).to_string());
        }
        if !final_output.is_empty() { return final_output; }
    }

    // 3. 通用 Key-Value 模式
    let mut kv_rows = Vec::new();
    for line in clean_lines {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().trim_start_matches("- ").to_string();
            let value = v.trim().to_string();
            if !key.is_empty() && !value.is_empty() {
                kv_rows.push(KeyValue { key, value });
            }
        }
    }
    if kv_rows.is_empty() { return processed_text; }
    Table::new(kv_rows).with(Style::modern()).to_string()
}

async fn process_query<E: llm_chain::traits::Executor>(
    chain: &mut Chain,
    exec: &E,
    tool_collection: &ToolCollection<RustPilotToolbox>,
    query: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut current_query = if is_confirmation(query) {
        format!("{}（用户确认继续执行下一步操作）", query)
    } else {
        query.to_string()
    };

    let mut turn_count = 0;
    let max_turns = 10; // 防止无限递归

    while turn_count < max_turns {
        turn_count += 1;
        
    // 创建用户查询步骤
        let step = Step::for_prompt_template(prompt!(user: &current_query));
    
    // 发送查询到LLM
    let response = chain.send_message(step, &parameters!(), exec).await?;
        let mut response_text = response.to_immediate().await?.as_content().to_string();
    
        // 隐形自检与重试循环（用于处理格式错误的 YAML）
        let mut retry_count = 0;
        let max_retries = 1;
        let mut executed_tool_name = String::new();
        
        let tool_call_result = loop {
            // 提取逻辑
            let clean_text = if let Some(start) = response_text.find("```yaml") {
                let rest = &response_text[start..];
                if let Some(end) = rest[7..].find("```") {
                    rest[..end + 10].to_string()
                } else {
                    rest.to_string()
                }
            } else if response_text.contains("command:") {
                response_text.trim().to_string()
            } else {
                // 如果没有检测到工具调用，直接返回 None
                break None;
            };

            // 提取工具名称用于后续显示逻辑
            executed_tool_name = clean_text.lines()
                .find(|l| l.trim().starts_with("command:"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_string())
                .unwrap_or_default();

            // 尝试解析并执行
            match tool_collection.process_chat_input(&clean_text).await {
                Ok(result) => break Some(result),
                Err(e) => {
                    use llm_chain::tools::ToolUseError;
                    
                    // 如果是工具执行错误，反馈给 AI 让它解释或决定下一步
                    if let ToolUseError::ToolError(_) = e {
                        break Some(format!("工具执行发生错误：{:?}", e));
                    }

                    // 如果是解析错误且看起来像工具调用，尝试重试一次
                    if response_text.contains("command:") && retry_count < max_retries {
                        retry_count += 1;
                        let retry_prompt = "系统提示：检测到工具调用格式错误。请重新输出工具调用，确保它是纯净的 YAML 代码块。";
                        let retry_step = Step::for_prompt_template(prompt!(user: retry_prompt));
                        let retry_response = chain.send_message(retry_step, &parameters!(), exec).await?;
                        response_text = retry_response.to_immediate().await?.as_content().to_string();
                        continue;
                    } else {
                        // 彻底解析失败
                        break None;
                    }
                }
            }
        };

        if let Some(tool_result) = tool_call_result {
            println!("{} {}", "助手:".cyan().bold(), "[正在调用工具处理您的请求...]".yellow());
            
            let result_str = tool_result.to_string();

            // 特殊逻辑：如果是 RssTool 且执行成功（不包含错误关键字），则隐藏明细表格
            let is_rss_success = executed_tool_name == "RssTool" && !result_str.contains("error: Some");
            
            if is_rss_success {
                println!("{}", "[新闻获取成功，正在为您整理摘要并翻译...]".bright_black().italic());
            } else {
                // 在终端显示精美的表格
                println!("{}\n{}", "工具执行结果:".blue().bold(), format_tool_output_as_table(&result_str));
            }
            
            // 将结果作为下一轮的输入（保持纯文本/YAML格式，方便AI理解）
            current_query = format!("工具执行结果：\n{}\n\n请根据此结果决定是继续执行下一个工具，还是给出最终回答。如果任务未完成，你可以继续调用下一个工具。", result_str);
            // 继续循环
            continue;
        } else {
            // 没有工具调用了，打印最终回答并退出循环
            println!("\n{} {}", "助手:".cyan().bold(), clean_output(&response_text));
            break;
    }
    }
    
    if turn_count >= max_turns {
        println!("\n{} {}", "助手:".cyan().bold(), "[警告：已达到最大任务步骤限制]".red().bold());
    }

    Ok(())
}

fn print_help() {
    println!("\n{}", "可用命令：".bold());
    println!("  {}          - 显示此帮助信息", "help".cyan());
    println!("  {}   - 退出程序", "exit / quit".cyan());
    println!("\n{}", "示例查询：".bold());
    println!("  - {}", "查找我昨天创建的文档".bright_black());
    println!("  - {}", "今天北京天气怎么样".bright_black());
    println!("  - {}", "查看系统CPU信息".bright_black());
    println!("  - {}", "列出当前目录的文件".bright_black());
    println!();
}