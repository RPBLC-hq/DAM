use super::*;
use std::{cell::RefCell, collections::VecDeque};

struct FakeRunner {
    outputs: RefCell<VecDeque<String>>,
    commands: RefCell<Vec<Vec<String>>>,
    fail_on_networksetup: bool,
}

impl FakeRunner {
    fn new(outputs: Vec<&str>) -> Self {
        Self {
            outputs: RefCell::new(outputs.into_iter().map(str::to_string).collect()),
            commands: RefCell::new(Vec::new()),
            fail_on_networksetup: false,
        }
    }

    fn failing(outputs: Vec<&str>) -> Self {
        Self {
            outputs: RefCell::new(outputs.into_iter().map(str::to_string).collect()),
            commands: RefCell::new(Vec::new()),
            fail_on_networksetup: true,
        }
    }
}

impl Runner for FakeRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<String, MacosNetworkError> {
        let mut command = vec![program.to_string()];
        command.extend(args.iter().map(|arg| (*arg).to_string()));
        self.commands.borrow_mut().push(command);
        if self.fail_on_networksetup
            && program == NETWORKSETUP
            && args.first().is_some_and(|arg| arg.starts_with("-set"))
        {
            return Err(MacosNetworkError::CommandFailed {
                program: program.to_string(),
                args: args.join(" "),
                status: "exit status: 1".to_string(),
                stderr: "synthetic failure".to_string(),
            });
        }
        Ok(self.outputs.borrow_mut().pop_front().unwrap_or_default())
    }
}

#[test]
fn parses_network_services_without_disabled_entries() {
    let services = parse_network_services(
        "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\n*Bluetooth PAN\nUSB 10/100/1000 LAN\n",
    );

    assert_eq!(services, vec!["Wi-Fi", "USB 10/100/1000 LAN"]);
}

#[test]
fn parses_auto_proxy_state() {
    let output = "URL: file:///tmp/dam.pac\nEnabled: Yes\n";

    assert!(parse_enabled(output));
    assert_eq!(parse_url(output), Some("file:///tmp/dam.pac".to_string()));
}

#[test]
fn pac_routes_proxyable_http_traffic_with_local_bypass() {
    let pac = pac_content("http://127.0.0.1:7828");

    assert!(pac.contains("protected-host: api.openai.com"));
    assert!(pac.contains("PROXY 127.0.0.1:7828"));
    assert!(pac.contains("DIRECT"));
    assert!(pac.contains("url.substring(0, 5) === \"http:\""));
    assert!(pac.contains("host === \"localhost\""));
    assert!(pac.contains("shExpMatch(host, \"192.168.*\")"));
    assert!(pac.contains("bareHost === \"::1\""));
    assert!(pac.contains("shExpMatch(bareHost, \"fe8*:*\")"));
    assert!(pac.contains("shExpMatch(bareHost, \"fc*:*\")"));
}

#[test]
fn pac_accepts_configured_protected_hosts() {
    let pac = pac_content_for_hosts(
        "http://127.0.0.1:7828",
        &[
            "https://api.enterprise-ai.example/v1".to_string(),
            "API.ENTERPRISE-AI.EXAMPLE:443".to_string(),
        ],
    );

    assert!(pac.contains("protected-host: api.enterprise-ai.example"));
    assert_eq!(pac.matches("api.enterprise-ai.example").count(), 1);
    assert!(!pac.contains("api.openai.com"));
}

#[test]
fn file_url_percent_encodes_paths_for_pac_settings() {
    let path = PathBuf::from("/Users/DAM Tester/.dam/network/macos-system-proxy/dam ai.pac");

    assert_eq!(
        file_url(&path),
        "file:///Users/DAM%20Tester/.dam/network/macos-system-proxy/dam%20ai.pac"
    );
}

#[test]
fn install_plan_records_prior_service_states_and_commands() {
    let runner = FakeRunner::new(vec![
        "Wi-Fi\nUSB LAN\n",
        "URL: file:///old.pac\nEnabled: Yes\n",
        "URL:\nEnabled: No\n",
    ]);
    let dir = tempfile::tempdir().unwrap();

    let plan = install_plan(dir.path(), "http://127.0.0.1:7828", &runner).unwrap();

    assert!(plan.can_execute || plan.support == MacosSystemProxySupport::Planned);
    assert!(plan.protected_hosts.contains(&"api.openai.com".to_string()));
    assert_eq!(plan.services.len(), 2);
    assert_eq!(plan.commands.len(), 4);
    assert_eq!(plan.commands[0].args[0], "-setautoproxyurl");
    assert_eq!(plan.commands[1].args[0], "-setautoproxystate");
    assert_eq!(plan.commands[1].args[2], "on");
}

#[test]
fn apply_writes_rollback_after_route_commands_and_remove_restores() {
    let runner = FakeRunner::new(vec![
        "Wi-Fi\n",
        "URL: file:///old.pac\nEnabled: Yes\n",
        "",
        "",
        "",
        "",
    ]);
    let dir = tempfile::tempdir().unwrap();

    let installed =
        install_system_proxy_with_runner(dir.path(), "http://127.0.0.1:7828", &runner).unwrap();

    if support() == MacosSystemProxySupport::Implemented {
        assert_eq!(installed.state, MacosSystemProxyResultState::Installed);
        assert!(installed.plan.paths.rollback_path.exists());
        assert!(installed.plan.paths.pac_path.exists());

        let removed =
            remove_system_proxy_with_runner(dir.path(), "http://127.0.0.1:7828", &runner).unwrap();

        assert_eq!(removed.state, MacosSystemProxyResultState::Removed);
        assert!(!removed.plan.paths.rollback_path.exists());
        assert!(!removed.plan.paths.pac_path.exists());
        let commands = runner.commands.borrow();
        assert!(
            commands
                .iter()
                .any(|command| command.contains(&"-setautoproxyurl".to_string()))
        );
        assert!(
            commands
                .iter()
                .any(|command| command.contains(&"-setautoproxystate".to_string()))
        );
    } else {
        assert_eq!(
            installed.state,
            MacosSystemProxyResultState::AlreadyInstalled
        );
    }
}

#[test]
fn install_failure_does_not_leave_rollback_marker() {
    let runner = FakeRunner::failing(vec!["Wi-Fi\n", "URL: file:///old.pac\nEnabled: Yes\n"]);
    let dir = tempfile::tempdir().unwrap();

    let result = install_system_proxy_with_runner(dir.path(), "http://127.0.0.1:7828", &runner);

    if support() == MacosSystemProxySupport::Implemented {
        assert!(result.is_err());
        let paths = MacosNetworkPaths::for_state_dir(dir.path());
        assert!(!paths.rollback_path.exists());
    }
}
