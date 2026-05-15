use super::*;

#[test]
fn non_empty_message_uses_fallback_for_blank_messages() {
    assert_eq!(non_empty_message(String::new(), "fallback"), "fallback");
    assert_eq!(non_empty_message("ready".to_string(), "fallback"), "ready");
}

#[test]
fn parses_enabled_system_extension_as_ready() {
    let output = concat!(
        "enabled\tactive\tteamID\tbundleID (version)\tname\t[state]\n",
        "*\t*\t2T6856RWGV\tcom.rpblc.dam.network-extension (1.0/1)\tDAM Network Protection\t[activated enabled]\n",
    );

    assert_eq!(
        parse_systemextensionsctl_outcome(output, "com.rpblc.dam.network-extension"),
        Some(ActivationOutcome::Ready(
            "DAM Network Protection is active".to_string()
        ))
    );
}

#[test]
fn enabled_system_extension_with_stale_build_requires_activation() {
    let output = concat!(
        "enabled\tactive\tteamID\tbundleID (version)\tname\t[state]\n",
        "*\t*\t2T6856RWGV\tcom.rpblc.dam.network-extension (1.0/1)\tDAM Network Protection\t[activated enabled]\n",
    );

    assert_eq!(
        parse_systemextensionsctl_outcome_with_bundled_build(
            output,
            "com.rpblc.dam.network-extension",
            Some(2)
        ),
        None
    );
}

#[test]
fn parses_systemextensionsctl_build_number() {
    assert_eq!(
        parse_systemextensionsctl_build(
            "*\t*\t2T6856RWGV\tcom.rpblc.dam.network-extension (1.0.1/2)\tDAM Network Protection\t[activated enabled]"
        ),
        Some(2)
    );
}

#[test]
fn parses_plist_string_values() {
    let xml = "<plist><dict><key>CFBundleVersion</key><string>2</string></dict></plist>";

    assert_eq!(
        parse_plist_string_value(xml, "CFBundleVersion"),
        Some("2".to_string())
    );
}

#[test]
fn parses_waiting_for_user_system_extension_as_needs_approval() {
    let output = concat!(
        "enabled\tactive\tteamID\tbundleID (version)\tname\t[state]\n",
        "\t*\t2T6856RWGV\tcom.rpblc.dam.network-extension (1.0/1)\tDAM Network Protection\t[activated waiting for user]\n",
    );

    assert_eq!(
        parse_systemextensionsctl_outcome(output, "com.rpblc.dam.network-extension"),
        Some(ActivationOutcome::NeedsApproval(
            APPROVAL_MESSAGE.to_string()
        ))
    );
}

#[test]
fn ignores_other_system_extension_states() {
    let output = concat!(
        "enabled\tactive\tteamID\tbundleID (version)\tname\t[state]\n",
        "\t\t2T6856RWGV\tcom.rpblc.other.network-extension (1.0/1)\tOther Network Protection\t[terminated waiting to uninstall on reboot]\n",
    );

    assert_eq!(
        parse_systemextensionsctl_outcome(output, "com.rpblc.dam.network-extension"),
        None
    );
}

#[test]
fn parses_reboot_system_extension_state() {
    let output = concat!(
        "enabled\tactive\tteamID\tbundleID (version)\tname\t[state]\n",
        "*\t\t2T6856RWGV\tcom.rpblc.dam.network-extension (1.0/1)\tDAM Network Protection\t[activated waiting to reboot]\n",
    );

    assert_eq!(
        parse_systemextensionsctl_outcome(output, "com.rpblc.dam.network-extension"),
        Some(ActivationOutcome::NeedsReboot(REBOOT_MESSAGE.to_string()))
    );
}

#[test]
fn parses_uninstall_reboot_system_extension_state() {
    let output = concat!(
        "enabled\tactive\tteamID\tbundleID (version)\tname\t[state]\n",
        "\t\t2T6856RWGV\tcom.rpblc.dam.network-extension (1.0/1)\tDAM Network Protection\t[terminated waiting to uninstall on reboot]\n",
    );

    assert_eq!(
        parse_systemextensionsctl_outcome(output, "com.rpblc.dam.network-extension"),
        Some(ActivationOutcome::NeedsReboot(REBOOT_MESSAGE.to_string()))
    );
}
