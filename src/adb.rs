#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub serial: String,
    pub state: String,
    pub product: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
    pub transport_id: Option<String>,
}

impl DeviceInfo {
    pub fn is_ready(&self) -> bool {
        self.state == "device"
    }

    pub fn display_name(&self) -> String {
        match &self.model {
            Some(model) if !model.is_empty() => format!("{} ({})", self.serial, model),
            _ => self.serial.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageScope {
    ThirdParty,
    All,
    System,
}

impl PackageScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::ThirdParty => "第三方应用",
            Self::All => "全部应用",
            Self::System => "系统应用",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub reinstall: bool,
    pub downgrade: bool,
    pub grant_permissions: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            reinstall: true,
            downgrade: false,
            grant_permissions: false,
        }
    }
}

pub fn start_server_args() -> Vec<String> {
    strings(["start-server"])
}

pub fn devices_args() -> Vec<String> {
    strings(["devices", "-l"])
}

pub fn connect_args(target: &str) -> Vec<String> {
    vec!["connect".to_owned(), target.trim().to_owned()]
}

pub fn disconnect_args(target: Option<&str>) -> Vec<String> {
    let mut args = strings(["disconnect"]);
    if let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) {
        args.push(target.to_owned());
    }
    args
}

pub fn package_args(device_serial: Option<&str>, scope: PackageScope) -> Vec<String> {
    let mut args = with_device(device_serial);
    args.extend(strings(["shell", "pm", "list", "packages"]));

    match scope {
        PackageScope::ThirdParty => args.push("-3".to_owned()),
        PackageScope::System => args.push("-s".to_owned()),
        PackageScope::All => {}
    }

    args
}

pub fn install_args(
    device_serial: Option<&str>,
    apk_path: &str,
    options: &InstallOptions,
) -> Vec<String> {
    let mut args = with_device(device_serial);
    args.push("install".to_owned());

    if options.reinstall {
        args.push("-r".to_owned());
    }
    if options.downgrade {
        args.push("-d".to_owned());
    }
    if options.grant_permissions {
        args.push("-g".to_owned());
    }

    args.push(apk_path.trim().to_owned());
    args
}

pub fn parse_devices(output: &str) -> Vec<DeviceInfo> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("List of devices") || line.starts_with('*') {
                return None;
            }

            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_owned();
            let state = parts.next().unwrap_or("unknown").to_owned();

            let mut product = None;
            let mut model = None;
            let mut device = None;
            let mut transport_id = None;

            for token in parts {
                if let Some((key, value)) = token.split_once(':') {
                    match key {
                        "product" => product = Some(value.to_owned()),
                        "model" => model = Some(value.replace('_', " ")),
                        "device" => device = Some(value.to_owned()),
                        "transport_id" => transport_id = Some(value.to_owned()),
                        _ => {}
                    }
                }
            }

            Some(DeviceInfo {
                serial,
                state,
                product,
                model,
                device,
                transport_id,
            })
        })
        .collect()
}

pub fn parse_packages(output: &str) -> Vec<String> {
    let mut packages = output
        .lines()
        .filter_map(|line| {
            let payload = line.trim().strip_prefix("package:")?.trim();
            let name = payload
                .rsplit_once('=')
                .map_or(payload, |(_, name)| name)
                .trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect::<Vec<_>>();

    packages.sort();
    packages.dedup();
    packages
}

fn with_device(device_serial: Option<&str>) -> Vec<String> {
    match device_serial
        .map(str::trim)
        .filter(|serial| !serial.is_empty())
    {
        Some(serial) => strings(["-s", serial]),
        None => Vec::new(),
    }
}

fn strings<const N: usize>(items: [&str; N]) -> Vec<String> {
    items.into_iter().map(ToOwned::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adb_devices_l_output() {
        let output = r#"
List of devices attached
R5CT123ABCD device product:o1q model:SM_G9910 device:o1q transport_id:3
192.168.1.8:5555 offline
emulator-5554 unauthorized
"#;

        let devices = parse_devices(output);
        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].serial, "R5CT123ABCD");
        assert_eq!(devices[0].state, "device");
        assert_eq!(devices[0].model.as_deref(), Some("SM G9910"));
        assert_eq!(devices[1].state, "offline");
        assert_eq!(devices[2].state, "unauthorized");
    }

    #[test]
    fn parses_package_names_from_plain_and_file_modes() {
        let output = r#"
package:com.example.alpha
package:/data/app/~~hash/base.apk=com.example.beta
package:com.example.alpha
"#;

        let packages = parse_packages(output);
        assert_eq!(packages, vec!["com.example.alpha", "com.example.beta"]);
    }
}
