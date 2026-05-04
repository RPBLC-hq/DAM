use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationOutcome {
    Ready(String),
    NeedsApproval(String),
}

const RETURN_READY: i32 = 0;
const RETURN_NEEDS_APPROVAL: i32 = 1;
const RETURN_FAILED: i32 = 2;
const RETURN_INVALID_ARGUMENT: i32 = 3;
const RETURN_TIMED_OUT: i32 = 4;

unsafe extern "C" {
    fn dam_tray_activate_system_extension(
        bundle_identifier: *const c_char,
        timeout_seconds: f64,
        message_buffer: *mut c_char,
        message_buffer_len: usize,
    ) -> i32;
}

pub fn activate(bundle_identifier: &str, timeout: Duration) -> Result<ActivationOutcome, String> {
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
            "approve DAM Network Protection in System Settings, then click Connect again",
        ))),
        RETURN_INVALID_ARGUMENT => Err(non_empty_message(
            message,
            "invalid System Extension activation request",
        )),
        RETURN_TIMED_OUT => Err(non_empty_message(
            message,
            "macOS did not register the DAM Network Protection activation request",
        )),
        RETURN_FAILED => Err(non_empty_message(
            message,
            "DAM Network Protection activation failed",
        )),
        _ => Err(non_empty_message(
            message,
            "DAM Network Protection activation returned an unknown result",
        )),
    }
}

fn non_empty_message(message: String, fallback: &str) -> String {
    if message.is_empty() {
        fallback.to_string()
    } else {
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_message_uses_fallback_for_blank_messages() {
        assert_eq!(non_empty_message(String::new(), "fallback"), "fallback");
        assert_eq!(non_empty_message("ready".to_string(), "fallback"), "ready");
    }
}
