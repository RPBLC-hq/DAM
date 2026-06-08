use std::{
    env,
    ffi::{CStr, CString},
    fs,
    os::raw::c_char,
    path::PathBuf,
    process::Command,
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationOutcome {
    Ready(String),
    NeedsApproval(String),
    NeedsReboot(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeactivationOutcome {
    Removed(String),
    NeedsApproval(String),
    NeedsReboot(String),
}

const RETURN_READY: i32 = 0;
const RETURN_NEEDS_APPROVAL: i32 = 1;
const RETURN_FAILED: i32 = 2;
const RETURN_INVALID_ARGUMENT: i32 = 3;
const RETURN_TIMED_OUT: i32 = 4;
const RETURN_NEEDS_REBOOT: i32 = 5;
const APPROVAL_MESSAGE: &str =
    "approve DAM Network Protection in System Settings, then click Connect/Resume again";
const REBOOT_MESSAGE: &str = "restart macOS to finish the DAM Network Protection system change";

unsafe extern "C" {
    fn dam_tray_activate_system_extension(
        bundle_identifier: *const c_char,
        timeout_seconds: f64,
        message_buffer: *mut c_char,
        message_buffer_len: usize,
    ) -> i32;
    fn dam_tray_deactivate_system_extension(
        bundle_identifier: *const c_char,
        timeout_seconds: f64,
        message_buffer: *mut c_char,
        message_buffer_len: usize,
    ) -> i32;
}

pub fn activate(bundle_identifier: &str, timeout: Duration) -> Result<ActivationOutcome, String> {
    if let Some(outcome) = installed_extension_outcome(bundle_identifier) {
        match outcome {
            ActivationOutcome::Ready(_) | ActivationOutcome::NeedsReboot(_) => return Ok(outcome),
            ActivationOutcome::NeedsApproval(_) => {
                // `activated waiting for user` can survive from an
                // earlier app session, but Apple's approval path only
                // completes while the requesting app retains a live
                // OSSystemExtensionRequest. Submit again from DAM.app
                // so opening Settings is paired with a live request.
            }
        }
    }

    let bundle_identifier = CString::new(bundle_identifier)
        .map_err(|_| "System Extension bundle identifier contains a null byte".to_string())?;
    let mut message = vec![0 as c_char; 2048];
    let status = unsafe {
        dam_tray_activate_system_extension(
            bundle_identifier.as_ptr(),
            timeout.as_secs_f64(),
            message.as_mut_ptr(),
            message.len(),
        )
    };
    let message = unsafe { CStr::from_ptr(message.as_ptr()) }
        .to_string_lossy()
        .trim()
        .to_string();

    match status {
        RETURN_READY => Ok(ActivationOutcome::Ready(non_empty_message(
            message,
            "DAM Network Protection is active",
        ))),
        RETURN_NEEDS_APPROVAL => Ok(ActivationOutcome::NeedsApproval(non_empty_message(
            message,
            APPROVAL_MESSAGE,
        ))),
        RETURN_NEEDS_REBOOT => Ok(ActivationOutcome::NeedsReboot(non_empty_message(
            message,
            REBOOT_MESSAGE,
        ))),
        RETURN_INVALID_ARGUMENT => Err(non_empty_message(
            message,
            "invalid System Extension activation request",
        )),
        RETURN_TIMED_OUT => Err(non_empty_message(
            message,
            "macOS did not register the DAM Network Protection activation request",
        )),
        RETURN_FAILED => {
            installed_extension_outcome(bundle_identifier.to_str().unwrap_or_default())
                .map(Ok)
                .unwrap_or_else(|| {
                    Err(non_empty_message(
                        message,
                        "DAM Network Protection activation failed",
                    ))
                })
        }
        _ => Err(non_empty_message(
            message,
            "DAM Network Protection activation returned an unknown result",
        )),
    }
}

pub fn deactivate(
    bundle_identifier: &str,
    timeout: Duration,
) -> Result<DeactivationOutcome, String> {
    let bundle_identifier = CString::new(bundle_identifier)
        .map_err(|_| "System Extension bundle identifier contains a null byte".to_string())?;
    let mut message = vec![0 as c_char; 2048];
    let status = unsafe {
        dam_tray_deactivate_system_extension(
            bundle_identifier.as_ptr(),
            timeout.as_secs_f64(),
            message.as_mut_ptr(),
            message.len(),
        )
    };
    let message = unsafe { CStr::from_ptr(message.as_ptr()) }
        .to_string_lossy()
        .trim()
        .to_string();

    match status {
        RETURN_READY => Ok(DeactivationOutcome::Removed(non_empty_message(
            message,
            "DAM Network Protection is deactivated",
        ))),
        RETURN_NEEDS_APPROVAL => Ok(DeactivationOutcome::NeedsApproval(non_empty_message(
            message,
            "approve removing DAM Network Protection in System Settings",
        ))),
        RETURN_NEEDS_REBOOT => Ok(DeactivationOutcome::NeedsReboot(non_empty_message(
            message,
            "DAM Network Protection will finish uninstalling after reboot",
        ))),
        RETURN_INVALID_ARGUMENT => Err(non_empty_message(
            message,
            "invalid System Extension deactivation request",
        )),
        RETURN_TIMED_OUT => Err(non_empty_message(
            message,
            "macOS did not register the DAM Network Protection deactivation request",
        )),
        RETURN_FAILED => Err(non_empty_message(
            message,
            "DAM Network Protection deactivation failed",
        )),
        _ => Err(non_empty_message(
            message,
            "DAM Network Protection deactivation returned an unknown result",
        )),
    }
}

fn installed_extension_outcome(bundle_identifier: &str) -> Option<ActivationOutcome> {
    let output = Command::new("/usr/bin/systemextensionsctl")
        .arg("list")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_systemextensionsctl_outcome_with_bundled_build(
        &stdout,
        bundle_identifier,
        bundled_system_extension_build(bundle_identifier),
    )
}

#[cfg(test)]
fn parse_systemextensionsctl_outcome(
    output: &str,
    bundle_identifier: &str,
) -> Option<ActivationOutcome> {
    parse_systemextensionsctl_outcome_with_bundled_build(output, bundle_identifier, None)
}

fn parse_systemextensionsctl_outcome_with_bundled_build(
    output: &str,
    bundle_identifier: &str,
    bundled_build: Option<u64>,
) -> Option<ActivationOutcome> {
    let lines: Vec<&str> = output
        .lines()
        .filter(|line| {
            line.split_whitespace()
                .any(|part| part == bundle_identifier)
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    if lines.iter().any(|line| {
        line.contains("[activated enabled]") && !installed_build_is_stale(line, bundled_build)
    }) {
        return Some(ActivationOutcome::Ready(
            "DAM Network Protection is active".to_string(),
        ));
    }
    if lines
        .iter()
        .any(|line| line.contains("[activated waiting for user]"))
    {
        return Some(ActivationOutcome::NeedsApproval(
            APPROVAL_MESSAGE.to_string(),
        ));
    }
    if lines
        .iter()
        .any(|line| line.contains("waiting") && line.contains("reboot"))
    {
        return Some(ActivationOutcome::NeedsReboot(REBOOT_MESSAGE.to_string()));
    }
    None
}

fn installed_build_is_stale(systemextensionsctl_line: &str, bundled_build: Option<u64>) -> bool {
    let Some(bundled_build) = bundled_build else {
        return false;
    };
    parse_systemextensionsctl_build(systemextensionsctl_line)
        .map(|installed_build| installed_build < bundled_build)
        .unwrap_or(false)
}

fn parse_systemextensionsctl_build(line: &str) -> Option<u64> {
    let version = line.split_once('(')?.1.split_once(')')?.0;
    let build = version.split_once('/')?.1;
    build.parse().ok()
}

fn bundled_system_extension_build(bundle_identifier: &str) -> Option<u64> {
    let info_plist = bundled_system_extension_info_plist(bundle_identifier)?;
    let xml = fs::read_to_string(info_plist).ok()?;
    parse_plist_string_value(&xml, "CFBundleVersion")?
        .parse()
        .ok()
}

fn bundled_system_extension_info_plist(bundle_identifier: &str) -> Option<PathBuf> {
    let macos_dir = env::current_exe().ok()?.parent()?.to_path_buf();
    let contents_dir = macos_dir.parent()?;
    Some(
        contents_dir
            .join("Library")
            .join("SystemExtensions")
            .join(format!("{bundle_identifier}.systemextension"))
            .join("Contents")
            .join("Info.plist"),
    )
}

fn parse_plist_string_value(xml: &str, key: &str) -> Option<String> {
    let key_marker = format!("<key>{key}</key>");
    let after_key = xml.split_once(&key_marker)?.1;
    let after_string = after_key.split_once("<string>")?.1;
    let value = after_string.split_once("</string>")?.0;
    Some(value.trim().to_string())
}

fn non_empty_message(message: String, fallback: &str) -> String {
    if message.is_empty() {
        fallback.to_string()
    } else {
        message
    }
}

#[cfg(test)]
#[path = "macos_system_extension_tests.rs"]
mod tests;
