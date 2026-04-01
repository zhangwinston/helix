//! 演示 IME 平台抽象层功能的示例
//!
//! 运行: `cargo run --example ime_platform_demo`

use helix_term::handlers::ime::platform::{self, ImeDetector, ImeType, ImeSettings};
use std::io::{self, Write};

fn main() -> anyhow::Result<()> {
    // 初始化日志
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    println!("=== IME 平台抽象层功能演示 ===\n");

    // 1. 检测平台 IME 支持
    println!("1. 平台 IME 支持检测:");
    println!("   IME 可用: {}", platform::is_ime_available());

    // 2. 获取 IME 信息
    println!("\n2. 当前 IME 信息:");
    match platform::get_ime_info() {
        Ok(info) => {
            println!("   名称: {}", info.name);
            println!("   能力: {:?}", info.capabilities);
            if let Some(version) = info.version {
                println!("   版本: {}", version);
            }

            // 3. IME 类型检测
            let ime_type = ImeDetector::detect_ime_type(&info.name);
            println!("\n3. IME 类型检测:");
            println!("   检测到的类型: {:?}", ime_type);

            // 4. 平台特定设置
            let settings = ImeDetector::get_optimal_settings(ime_type);
            println!("\n4. 优化设置:");
            println!("   重试次数: {}", settings.retry_count);
            println!("   重试延迟: {}ms", settings.retry_delay_ms);
            println!("   重置阈值: {}", settings.reset_threshold);
            if !settings.custom_settings.is_empty() {
                println!("   自定义设置:");
                for (key, value) in &settings.custom_settings {
                    println!("     {}: {}", key, value);
                }
            }
        }
        Err(e) => {
            println!("   错误: {}", e);
        }
    }

    // 5. 测试所有支持的 IME 类型检测
    println!("\n5. IME 类型检测测试:");
    let test_cases = vec![
        "搜狗拼音",
        "Microsoft IME",
        "Google Pinyin",
        "fcitx",
        "IBus",
        "百度输入法",
        "QQ拼音",
        "未知输入法",
    ];

    for ime_name in test_cases {
        let detected = ImeDetector::detect_ime_type(ime_name);
        let settings = ImeDetector::get_optimal_settings(detected);
        println!("   {}: {:?} (重试 {} 次)",
            ime_name, detected, settings.retry_count);
    }

    // 6. 初始化平台支持
    println!("\n6. 初始化平台支持:");
    match platform::initialize() {
        Ok(()) => println!("   ✓ 初始化成功"),
        Err(e) => println!("   ✗ 初始化失败: {}", e),
    }

    // 7. 测试 IME 状态操作
    println!("\n7. IME 状态操作:");
    if platform::is_ime_available() {
        match platform::is_ime_enabled() {
            Ok(state) => {
                println!("   当前状态: {}", if state { "开启" } else { "关闭" });

                // 询问是否要切换状态
                print!("   是否切换 IME 状态? [Y/n]: ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim().is_empty() || input.trim().to_lowercase() == "y" {
                    let target = !state;
                    match platform::set_ime_enabled(target) {
                        Ok(()) => {
                            println!("   ✓ 已切换到: {}", if target { "开启" } else { "关闭" });
                        }
                        Err(e) => {
                            println!("   ✗ 切换失败: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                println!("   ✗ 无法获取状态: {}", e);
            }
        }

        // 测试重置
        println!("\n8. IME 重置:");
        match platform::reset_if_needed() {
            Ok(()) => println!("   ✓ 重置完成"),
            Err(e) => println!("   ✗ 重置失败: {}", e),
        }
    } else {
        println!("   当前平台不支持 IME 或未安装 IME");
    }

    println!("\n=== 演示完成 ===");
    Ok(())
}