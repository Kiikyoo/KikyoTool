#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adb;
mod frida;
mod runner;

use std::fs;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use adb::{DeviceInfo, InstallOptions, PackageScope};
use eframe::egui;
use runner::CommandOutcome;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([1040.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "KikyoTool - Android Automation",
        options,
        Box::new(|creation_context| {
            let font_status = configure_fonts(&creation_context.egui_ctx);
            configure_style(&creation_context.egui_ctx);
            Ok(Box::new(KikyoApp::new(font_status)))
        }),
    )
}

fn configure_fonts(ctx: &egui::Context) -> String {
    let mut fonts = egui::FontDefinitions::default();

    for path in chinese_font_candidates() {
        if let Ok(bytes) = fs::read(path) {
            fonts.font_data.insert(
                "kikyo_cjk".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );

            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .insert(0, "kikyo_cjk".to_owned());
            }

            ctx.set_fonts(fonts);
            return format!("已加载中文字体：{path}");
        }
    }

    "未找到可用中文字体，若仍有乱码请安装或指定 Windows 中文字体。".to_owned()
}

fn chinese_font_candidates() -> &'static [&'static str] {
    &[
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\simsunb.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    ]
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_rgb(18, 21, 27);
    visuals.window_fill = egui::Color32::from_rgb(24, 28, 36);
    visuals.extreme_bg_color = egui::Color32::from_rgb(12, 14, 18);
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(28, 33, 42);
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(34, 40, 50);
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(47, 57, 70);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(55, 116, 161);
    visuals.selection.bg_fill = egui::Color32::from_rgb(65, 132, 184);
    visuals.window_corner_radius = egui::CornerRadius::same(14);
    visuals.menu_corner_radius = egui::CornerRadius::same(12);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(10);
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(10);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 9.0);
    style.spacing.window_margin = egui::Margin::same(14);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(22.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
    );
    ctx.set_style(style);
}

#[derive(Debug, Clone, Copy)]
enum JobKind {
    RefreshDevices,
    StartAndRefresh,
    ConnectTcp,
    DisconnectAdb,
    FetchPackages,
    InstallApk,
}

#[derive(Debug)]
struct JobResult {
    kind: JobKind,
    label: String,
    outcomes: Vec<CommandOutcome>,
    devices: Option<Vec<DeviceInfo>>,
    packages: Option<Vec<String>>,
}

impl JobResult {
    fn new(kind: JobKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            outcomes: Vec::new(),
            devices: None,
            packages: None,
        }
    }

    fn push(&mut self, outcome: CommandOutcome) {
        self.outcomes.push(outcome);
    }

    fn success(&self) -> bool {
        !self.outcomes.is_empty() && self.outcomes.iter().all(|outcome| outcome.success)
    }

    fn combined_output(&self) -> String {
        self.outcomes
            .iter()
            .map(format_outcome)
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn should_open_terminal(&self) -> bool {
        matches!(self.kind, JobKind::InstallApk)
    }
}

#[derive(Debug, Clone, Copy)]
enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

struct KikyoApp {
    adb_path: String,
    tcp_target: String,
    selected_device: Option<String>,
    devices: Vec<DeviceInfo>,

    package_scope: PackageScope,
    package_filter: String,
    packages: Vec<String>,
    selected_package: Option<String>,

    apk_path: String,
    install_options: InstallOptions,

    frida_bin: String,
    frida_ps_bin: String,
    frida_trace_bin: String,
    frida_package: String,
    frida_script_path: String,

    last_status_level: LogLevel,
    last_status: String,
    running_jobs: usize,
    tx: Sender<JobResult>,
    rx: Receiver<JobResult>,
}

impl Default for KikyoApp {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();

        Self {
            adb_path:
                r"C:\Users\Administrator\Downloads\Root\platform-tools-latest-windows\platform-tools\adb.exe"
                    .to_owned(),
            tcp_target: "127.0.0.1:12345".to_owned(),
            selected_device: None,
            devices: Vec::new(),

            package_scope: PackageScope::ThirdParty,
            package_filter: String::new(),
            packages: Vec::new(),
            selected_package: None,

            apk_path: String::new(),
            install_options: InstallOptions::default(),

            frida_bin: "frida".to_owned(),
            frida_ps_bin: "frida-ps".to_owned(),
            frida_trace_bin: "frida-trace".to_owned(),
            frida_package: String::new(),
            frida_script_path: "hook.js".to_owned(),

            last_status_level: LogLevel::Info,
            last_status: "就绪。配置 adb 后连接手机并刷新设备。".to_owned(),
            running_jobs: 0,
            tx,
            rx,
        }
    }
}

impl eframe::App for KikyoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_job_results();

        egui::SidePanel::left("codex_sidebar")
            .resizable(false)
            .exact_width(380.0)
            .frame(panel_frame(egui::Color32::from_rgb(16, 19, 25)))
            .show(ctx, |ui| self.draw_sidebar(ui));

        egui::SidePanel::right("frida_sidebar")
            .resizable(true)
            .default_width(360.0)
            .width_range(320.0..=480.0)
            .frame(panel_frame(egui::Color32::from_rgb(18, 21, 27)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        self.draw_frida_panel(ui);
                    });
            });

        egui::CentralPanel::default()
            .frame(panel_frame(egui::Color32::from_rgb(18, 21, 27)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        self.draw_main_workspace(ui);
                    });
            });

        if self.running_jobs > 0 {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }
}

impl KikyoApp {
    fn new(font_status: String) -> Self {
        let mut app = Self::default();
        app.push_log(LogLevel::Info, font_status);
        app
    }

    fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("KikyoTool")
                    .heading()
                    .strong()
                    .color(egui::Color32::from_rgb(235, 241, 248)),
            );
            ui.label(
                egui::RichText::new("Android ADB / APK / Frida 自动化工具台")
                    .color(egui::Color32::from_rgb(154, 166, 180)),
            );

            ui.add_space(18.0);
            ui.horizontal_wrapped(|ui| {
                if self.running_jobs > 0 {
                    ui.spinner();
                    status_pill(
                        ui,
                        &format!("{} 个任务运行中", self.running_jobs),
                        egui::Color32::from_rgb(90, 147, 196),
                    );
                } else {
                    status_pill(ui, "空闲", egui::Color32::from_rgb(79, 166, 117));
                }
                status_pill(
                    ui,
                    &format!(
                        "{} 台设备就绪",
                        self.devices
                            .iter()
                            .filter(|device| device.is_ready())
                            .count()
                    ),
                    egui::Color32::from_rgb(79, 166, 117),
                );
                status_pill(
                    ui,
                    &format!("{} 个包名", self.packages.len()),
                    egui::Color32::from_rgb(104, 132, 185),
                );
            });

            ui.add_space(8.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&self.last_status)
                        .color(log_level_color(self.last_status_level))
                        .small(),
                )
                .wrap(),
            );

            ui.add_space(18.0);
            section(ui, "ADB", |ui| {
                ui.label("ADB 路径");
                let adb_width = ui.available_width().max(180.0);
                ui.add(egui::TextEdit::singleline(&mut self.adb_path).desired_width(adb_width));
                if ui.add(secondary_button("选择 ADB")).clicked() {
                    self.pick_adb_path();
                }
            });

            ui.add_space(16.0);
            self.draw_device_panel(ui);
        });
    }

    fn draw_main_workspace(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("工作区")
                    .heading()
                    .strong()
                    .color(egui::Color32::from_rgb(235, 241, 248)),
            );
            ui.label(
                egui::RichText::new("APK 安装与 Frida 命令会在系统终端窗口中打开")
                    .color(egui::Color32::from_rgb(154, 166, 180)),
            );
        });

        ui.add_space(16.0);
        self.draw_package_panel(ui);
        ui.add_space(18.0);
        self.draw_install_panel(ui);
    }

    fn draw_device_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "设备连接", |ui| {
            ui.horizontal(|ui| {
                if ui.add(primary_button("有线连接")).clicked() {
                    self.start_and_refresh();
                }
                if ui.add(secondary_button("刷新设备")).clicked() {
                    self.refresh_devices();
                }
            });

            ui.add(egui::TextEdit::singleline(&mut self.tcp_target).desired_width(180.0));

            ui.horizontal(|ui| {
                let has_tcp_target = !self.tcp_target.trim().is_empty();
                if ui
                    .add_enabled(has_tcp_target, secondary_button("网络连接"))
                    .clicked()
                {
                    self.connect_tcp();
                }
                if ui.add(secondary_button("断开连接")).clicked() {
                    self.disconnect_adb();
                }
            });

            ui.add_space(8.0);

            ui.horizontal(|ui| {
                egui::ComboBox::from_label("目标设备")
                    .selected_text(self.selected_device_label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.selected_device, None, "默认 ADB 目标");
                        for device in &self.devices {
                            ui.selectable_value(
                                &mut self.selected_device,
                                Some(device.serial.clone()),
                                device.display_name(),
                            );
                        }
                    });

                if let Some(device) = self.selected_device_info() {
                    let state_text = if device.is_ready() {
                        egui::RichText::new("ready").color(egui::Color32::LIGHT_GREEN)
                    } else {
                        egui::RichText::new(&device.state).color(egui::Color32::YELLOW)
                    };
                    ui.label(state_text);
                }
            });

            if self.devices.is_empty() {
                ui.label("还没有加载到设备。");
            } else {
                egui::ScrollArea::horizontal()
                    .id_salt("device_grid_scroll")
                    .auto_shrink([false, true])
                    .max_width(ui.available_width())
                    .show(ui, |ui| {
                        egui::Grid::new("device_grid")
                            .striped(true)
                            .min_col_width(108.0)
                            .show(ui, |ui| {
                                ui.strong("序列号");
                                ui.strong("状态");
                                ui.strong("型号");
                                ui.strong("产品");
                                ui.strong("通道");
                                ui.end_row();

                                let devices = self.devices.clone();
                                for device in devices {
                                    let selected = self
                                        .selected_device
                                        .as_deref()
                                        .is_some_and(|serial| serial == device.serial);
                                    if ui.selectable_label(selected, &device.serial).clicked() {
                                        self.selected_device = Some(device.serial.clone());
                                    }
                                    ui.label(&device.state);
                                    ui.label(device.model.as_deref().unwrap_or("-"));
                                    ui.label(device.product.as_deref().unwrap_or("-"));
                                    ui.label(device.transport_id.as_deref().unwrap_or("-"));
                                    ui.end_row();
                                }
                            });
                    });
            }
        });
    }

    fn pick_adb_path(&mut self) {
        let mut dialog = rfd::FileDialog::new();

        #[cfg(target_os = "windows")]
        {
            dialog = dialog.add_filter("adb.exe", &["exe"]);
        }

        #[cfg(not(target_os = "windows"))]
        {
            dialog = dialog.add_filter("adb", &["adb"]);
        }

        if let Some(path) = dialog.pick_file() {
            self.adb_path = path.display().to_string();
        }
    }

    fn draw_package_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "应用包名", |ui| {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [42.0, 30.0],
                    egui::Label::new(
                        egui::RichText::new("范围")
                            .size(16.0)
                            .color(egui::Color32::from_rgb(170, 181, 194)),
                    ),
                );
                ui.add_space(10.0);
                for scope in [
                    PackageScope::ThirdParty,
                    PackageScope::All,
                    PackageScope::System,
                ] {
                    ui.selectable_value(
                        &mut self.package_scope,
                        scope,
                        egui::RichText::new(scope.label()).size(13.0),
                    );
                }
            });

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if ui.add(primary_button("一键获取包名")).clicked() {
                    self.fetch_packages();
                }
            });

            ui.horizontal(|ui| {
                ui.label("过滤");
                let filter_width = ui.available_width().max(160.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.package_filter)
                        .desired_width(filter_width)
                        .hint_text("com.example"),
                );
            });

            if let Some(package) = &self.selected_package {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("已选：").strong());
                    ui.add(
                        egui::Label::new(egui::RichText::new(package).strong())
                            .wrap()
                            .selectable(true),
                    );
                });
            }

            let filter = self.package_filter.trim().to_ascii_lowercase();
            let filtered = self
                .packages
                .iter()
                .filter(|package| {
                    filter.is_empty() || package.to_ascii_lowercase().contains(&filter)
                })
                .cloned()
                .collect::<Vec<_>>();

            ui.label(
                egui::RichText::new(format!("{} 个包名", filtered.len()))
                    .color(egui::Color32::from_rgb(154, 166, 180)),
            );
            egui::ScrollArea::vertical()
                .id_salt("package_list")
                .max_height(190.0)
                .show(ui, |ui| {
                    if filtered.is_empty() {
                        ui.label("暂无包名，点击“一键获取包名”后会显示在这里。");
                    }
                    for package in filtered {
                        let selected = self.selected_package.as_deref() == Some(package.as_str());
                        if ui.selectable_label(selected, &package).clicked() {
                            self.selected_package = Some(package.clone());
                            self.frida_package = package;
                        }
                    }
                });
        });
    }

    fn draw_install_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "APK 安装", |ui| {
            ui.horizontal(|ui| {
                ui.label("APK");
                let apk_width = ui.available_width().max(180.0);
                ui.add(egui::TextEdit::singleline(&mut self.apk_path).desired_width(apk_width));
            });

            ui.horizontal(|ui| {
                if ui.add(secondary_button("选择 APK")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Android APK", &["apk"])
                        .pick_file()
                    {
                        self.apk_path = path.display().to_string();
                    }
                }
                let can_install = !self.apk_path.trim().is_empty();
                if ui
                    .add_enabled(can_install, primary_button("执行 adb install"))
                    .clicked()
                {
                    self.install_apk();
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.install_options.reinstall, "覆盖安装 (-r)");
                ui.checkbox(&mut self.install_options.downgrade, "允许降级 (-d)");
                ui.checkbox(
                    &mut self.install_options.grant_permissions,
                    "授予运行时权限 (-g)",
                );
            });
        });
    }

    fn draw_frida_panel(&mut self, ui: &mut egui::Ui) {
        section(ui, "Frida 自动化命令", |ui| {
            let frida_input_width = frida_control_width(ui);
            frida_text_row(
                ui,
                "frida 客户端",
                &mut self.frida_bin,
                "",
                frida_input_width,
            );
            frida_text_row(
                ui,
                "进程列表",
                &mut self.frida_ps_bin,
                "",
                frida_input_width,
            );
            frida_text_row(
                ui,
                "Trace 工具",
                &mut self.frida_trace_bin,
                "",
                frida_input_width,
            );

            ui.add_space(4.0);

            let target_input_width = frida_control_width(ui);
            frida_text_row(
                ui,
                "目标包名",
                &mut self.frida_package,
                "选择包名后自动填充",
                target_input_width,
            );
            frida_text_row(
                ui,
                "Hook 脚本",
                &mut self.frida_script_path,
                "hook.js",
                target_input_width,
            );

            ui.horizontal(|ui| {
                frida_label_spacer(ui);
                ui.add_space(FRIDA_CONTROL_GAP);
                ui.allocate_ui_with_layout(
                    egui::vec2(target_input_width, 38.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let button_width =
                            ((target_input_width - ui.spacing().item_spacing.x) / 2.0).max(86.0);
                        if ui
                            .add_sized([button_width, 38.0], secondary_button("选择脚本"))
                            .clicked()
                        {
                            self.pick_frida_script_path();
                        }
                        if ui
                            .add_sized([button_width, 38.0], secondary_button("启动 CMD"))
                            .clicked()
                        {
                            self.open_frida_cmd();
                        }
                    },
                );
            });

            let templates = frida::build_templates(
                &self.frida_bin,
                &self.frida_ps_bin,
                &self.frida_trace_bin,
                &self.frida_package,
                &self.frida_script_path,
            );

            for template in templates {
                ui.separator();
                let mut command = template.command;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(template.title).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(secondary_button("复制")).clicked() {
                            ui.ctx().copy_text(command.clone());
                            self.push_log(LogLevel::Success, format!("已复制：{}", template.title));
                        }
                    });
                });
                ui.label(
                    egui::RichText::new(template.description)
                        .color(egui::Color32::from_rgb(154, 166, 180)),
                );
                let command_width = ui.available_width().max(180.0);
                let command_content_width =
                    (command.chars().count() as f32 * 7.4).clamp(command_width, 900.0);
                egui::ScrollArea::horizontal()
                    .id_salt(format!("frida_command_{}", template.title))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut command)
                                .desired_width(command_content_width)
                                .interactive(false),
                        );
                    });
            }
        });
    }

    fn start_and_refresh(&mut self) {
        let adb_path = self.adb_path.clone();
        self.spawn_job("启动 ADB 并刷新设备", move || {
            let mut result = JobResult::new(JobKind::StartAndRefresh, "启动 ADB 并刷新设备");
            result.push(runner::run_process(&adb_path, &adb::start_server_args()));
            let devices_outcome = runner::run_process(&adb_path, &adb::devices_args());
            if devices_outcome.success {
                result.devices = Some(adb::parse_devices(&devices_outcome.stdout));
            }
            result.push(devices_outcome);
            result
        });
    }

    fn refresh_devices(&mut self) {
        let adb_path = self.adb_path.clone();
        self.spawn_job("刷新设备", move || {
            let mut result = JobResult::new(JobKind::RefreshDevices, "刷新设备");
            let outcome = runner::run_process(&adb_path, &adb::devices_args());
            if outcome.success {
                result.devices = Some(adb::parse_devices(&outcome.stdout));
            }
            result.push(outcome);
            result
        });
    }

    fn connect_tcp(&mut self) {
        let adb_path = self.adb_path.clone();
        let target = self.tcp_target.clone();
        self.spawn_job(format!("连接 {target}"), move || {
            let mut result = JobResult::new(JobKind::ConnectTcp, format!("连接 {target}"));
            result.push(runner::run_process(&adb_path, &adb::connect_args(&target)));
            let devices_outcome = runner::run_process(&adb_path, &adb::devices_args());
            if devices_outcome.success {
                result.devices = Some(adb::parse_devices(&devices_outcome.stdout));
            }
            result.push(devices_outcome);
            result
        });
    }

    fn disconnect_adb(&mut self) {
        let adb_path = self.adb_path.clone();
        let target = self.tcp_target.clone();
        self.spawn_job("断开 ADB 连接", move || {
            let mut result = JobResult::new(JobKind::DisconnectAdb, "断开 ADB 连接");
            let outcome = runner::run_process(&adb_path, &adb::disconnect_args(Some(&target)));
            if outcome.success {
                result.devices = Some(Vec::new());
            }
            result.push(outcome);
            result
        });
    }

    fn fetch_packages(&mut self) {
        let adb_path = self.adb_path.clone();
        let device = self.selected_device.clone();
        let scope = self.package_scope;
        self.spawn_job(format!("获取{}", scope.label()), move || {
            let mut result =
                JobResult::new(JobKind::FetchPackages, format!("获取{}", scope.label()));
            let outcome =
                runner::run_process(&adb_path, &adb::package_args(device.as_deref(), scope));
            if outcome.success {
                result.packages = Some(adb::parse_packages(&outcome.stdout));
            }
            result.push(outcome);
            result
        });
    }

    fn install_apk(&mut self) {
        let adb_path = self.adb_path.clone();
        let device = self.selected_device.clone();
        let apk_path = self.apk_path.clone();
        let options = self.install_options.clone();
        self.spawn_job("安装 APK", move || {
            let mut result = JobResult::new(JobKind::InstallApk, "安装 APK");
            result.push(runner::run_process(
                &adb_path,
                &adb::install_args(device.as_deref(), &apk_path, &options),
            ));
            result
        });
    }

    fn open_frida_cmd(&mut self) {
        let commands = ["conda activate myenv"];
        match runner::open_cmd_with_commands(&commands) {
            Ok(()) => self.push_log(
                LogLevel::Success,
                format!("已打开 CMD：{}", commands.join("；")),
            ),
            Err(error) => self.push_log(LogLevel::Error, format!("无法打开 CMD：{error}")),
        }
    }

    fn pick_frida_script_path(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Frida Hook 脚本", &["js"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        {
            self.frida_script_path = path.display().to_string();
        }
    }

    fn spawn_job<F>(&mut self, label: impl Into<String>, job: F)
    where
        F: FnOnce() -> JobResult + Send + 'static,
    {
        let label = label.into();
        self.running_jobs += 1;
        self.push_log(LogLevel::Info, format!("开始：{label}"));

        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(job());
        });
    }

    fn handle_job_results(&mut self) {
        while let Ok(result) = self.rx.try_recv() {
            self.running_jobs = self.running_jobs.saturating_sub(1);

            match result.kind {
                JobKind::RefreshDevices
                | JobKind::StartAndRefresh
                | JobKind::ConnectTcp
                | JobKind::DisconnectAdb => {
                    if let Some(devices) = result.devices.clone() {
                        self.push_device_status_warnings(&devices);
                        self.devices = devices;
                        if let Some(selected) = &self.selected_device {
                            if !self.devices.iter().any(|device| device.serial == *selected) {
                                self.selected_device = None;
                            }
                        }
                    }
                }
                JobKind::FetchPackages => {
                    if let Some(packages) = result.packages.clone() {
                        self.packages = packages;
                        self.selected_package = None;
                    }
                }
                JobKind::InstallApk => {}
            }

            let level = if result.success() {
                LogLevel::Success
            } else {
                LogLevel::Error
            };

            if result.should_open_terminal() {
                let terminal_text = format!("{}\n\n{}", result.label, result.combined_output());
                if let Err(error) = runner::open_terminal_with_text(
                    &format!("KikyoTool - {}", result.label),
                    &terminal_text,
                ) {
                    self.push_log(LogLevel::Error, format!("无法打开终端窗口：{error}"));
                }
            }

            self.push_log(level, format!("完成：{}", result.label));
        }
    }

    fn selected_device_label(&self) -> String {
        self.selected_device
            .as_deref()
            .and_then(|serial| self.devices.iter().find(|device| device.serial == serial))
            .map(DeviceInfo::display_name)
            .unwrap_or_else(|| "默认 ADB 目标".to_owned())
    }

    fn selected_device_info(&self) -> Option<&DeviceInfo> {
        self.selected_device
            .as_deref()
            .and_then(|serial| self.devices.iter().find(|device| device.serial == serial))
    }

    fn push_log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.last_status_level = level;
        self.last_status = message
            .into()
            .lines()
            .next()
            .unwrap_or("状态已更新。")
            .to_owned();
    }

    fn push_device_status_warnings(&mut self, devices: &[DeviceInfo]) {
        if devices.is_empty() {
            self.push_log(
                LogLevel::Warning,
                "没有找到 Android 设备。请检查 USB 调试、数据线或 TCP/IP 目标。",
            );
            return;
        }

        for device in devices.iter().filter(|device| !device.is_ready()) {
            let hint = match device.state.as_str() {
                "unauthorized" => "请在手机上确认 USB 调试授权弹窗",
                "offline" => "尝试重连设备或重启 adb",
                _ => "执行 shell 命令前请先确认设备状态",
            };
            self.push_log(
                LogLevel::Warning,
                format!("{} 当前为 {}：{}。", device.serial, device.state, hint),
            );
        }
    }
}

fn panel_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::default()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(0))
        .inner_margin(egui::Margin::same(18))
}

fn section<R>(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    sized_section(ui, title, 0.0, add_contents)
}

fn sized_section<R>(
    ui: &mut egui::Ui,
    title: &str,
    min_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(26, 31, 39))
        .corner_radius(egui::CornerRadius::same(16))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(48, 57, 70)))
        .inner_margin(egui::Margin::same(18))
        .show(ui, |ui| {
            if min_height > 0.0 {
                ui.set_min_height(min_height);
            }
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .heading()
                        .strong()
                        .color(egui::Color32::from_rgb(232, 238, 246)),
                );
                ui.add_space(12.0);
                add_contents(ui)
            })
            .inner
        })
        .inner
}

const FRIDA_LABEL_WIDTH: f32 = 78.0;
const FRIDA_CONTROL_GAP: f32 = 12.0;

fn frida_control_width(ui: &egui::Ui) -> f32 {
    (ui.available_width() - FRIDA_LABEL_WIDTH - FRIDA_CONTROL_GAP).max(150.0)
}

fn frida_text_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    control_width: f32,
) {
    ui.horizontal(|ui| {
        frida_label(ui, label);
        ui.add_space(FRIDA_CONTROL_GAP);

        let mut edit = egui::TextEdit::singleline(value).desired_width(control_width);
        if !hint.is_empty() {
            edit = edit.hint_text(hint);
        }
        ui.add(edit);
    });
}

fn frida_label(ui: &mut egui::Ui, label: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(FRIDA_LABEL_WIDTH, 24.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.label(egui::RichText::new(label).color(egui::Color32::from_rgb(154, 166, 180)));
        },
    );
}

fn frida_label_spacer(ui: &mut egui::Ui) {
    ui.allocate_space(egui::vec2(FRIDA_LABEL_WIDTH, 24.0));
}

fn primary_button(text: &'static str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .strong()
            .color(egui::Color32::from_rgb(246, 249, 252)),
    )
    .fill(egui::Color32::from_rgb(56, 116, 169))
    .corner_radius(egui::CornerRadius::same(12))
    .min_size(egui::vec2(0.0, 38.0))
}

fn secondary_button(text: &'static str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(egui::Color32::from_rgb(226, 232, 240)))
        .fill(egui::Color32::from_rgb(39, 47, 59))
        .corner_radius(egui::CornerRadius::same(12))
        .min_size(egui::vec2(0.0, 38.0))
}

fn log_level_color(level: LogLevel) -> egui::Color32 {
    match level {
        LogLevel::Info => egui::Color32::from_rgb(154, 166, 180),
        LogLevel::Success => egui::Color32::from_rgb(126, 220, 150),
        LogLevel::Warning => egui::Color32::from_rgb(236, 198, 102),
        LogLevel::Error => egui::Color32::from_rgb(242, 133, 133),
    }
}

fn status_pill(ui: &mut egui::Ui, text: &str, accent: egui::Color32) {
    egui::Frame::default()
        .fill(egui::Color32::from_rgba_premultiplied(
            accent.r(),
            accent.g(),
            accent.b(),
            38,
        ))
        .stroke(egui::Stroke::new(1.0, accent))
        .corner_radius(egui::CornerRadius::same(24))
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .strong()
                    .color(egui::Color32::from_rgb(236, 242, 248)),
            );
        });
}

fn format_outcome(outcome: &CommandOutcome) -> String {
    let mut lines = vec![format!("$ {}", outcome.command_line())];

    match outcome.status_code {
        Some(code) => lines.push(format!("exit: {code}")),
        None => lines.push("exit: unavailable".to_owned()),
    }

    let stdout = outcome.stdout.trim();
    if !stdout.is_empty() {
        lines.push(stdout.to_owned());
    }

    let stderr = outcome.stderr.trim();
    if !stderr.is_empty() {
        lines.push(format!("stderr: {stderr}"));
    }

    lines.join("\n")
}
