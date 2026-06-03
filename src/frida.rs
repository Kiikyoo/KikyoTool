use crate::runner::quote_command;

#[derive(Debug, Clone)]
pub struct FridaTemplate {
    pub title: &'static str,
    pub description: &'static str,
    pub command: String,
}

pub fn frida_ps_args() -> Vec<String> {
    Vec::new()
}

pub fn build_templates(
    frida_bin: &str,
    frida_ps_bin: &str,
    frida_trace_bin: &str,
    package_name: &str,
    script_path: &str,
) -> Vec<FridaTemplate> {
    let package = non_empty_or(package_name, "<package.name>");
    let script = non_empty_or(script_path, "hook.js");

    let ps_args = frida_ps_args();
    let spawn_args = vec![
        "-U".to_owned(),
        "-f".to_owned(),
        package.clone(),
        "-l".to_owned(),
        script,
    ];
    let attach_args = vec!["-U".to_owned(), "-n".to_owned(), package.clone()];
    let trace_args = vec![
        "-U".to_owned(),
        "-f".to_owned(),
        package,
        "-i".to_owned(),
        "Java_*".to_owned(),
    ];

    vec![
        FridaTemplate {
            title: "列出设备应用",
            description: "查看 USB 设备上可附加的应用与进程。",
            command: quote_command(frida_ps_bin, &ps_args),
        },
        FridaTemplate {
            title: "Spawn 启动并注入",
            description: "启动目标应用，同时加载本地 Hook 脚本。",
            command: quote_command(frida_bin, &spawn_args),
        },
        FridaTemplate {
            title: "Attach 运行中进程",
            description: "附加到已运行的目标应用，不重启应用。",
            command: quote_command(frida_bin, &attach_args),
        },
        FridaTemplate {
            title: "Trace 方法调用",
            description: "预留 frida-trace 入口，可替换匹配规则。",
            command: quote_command(frida_trace_bin, &trace_args),
        },
    ]
}

fn non_empty_or(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}
