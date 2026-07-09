pub const EXTERNAL_TOOLS_ENV: &str = "OPENKOTO_ENABLE_EXTERNAL_TOOLS";

const DISABLED_MESSAGE: &str =
    "Phase 1 external tools are disabled. Set OPENKOTO_ENABLE_EXTERNAL_TOOLS=1 to enable this command for the current session.";

pub fn external_tools_enabled() -> bool {
    std::env::var(EXTERNAL_TOOLS_ENV)
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        })
        .unwrap_or(false)
}

pub fn require_external_tools_enabled(command_name: &str) -> Result<(), String> {
    if external_tools_enabled() {
        Ok(())
    } else {
        Err(format!("{command_name}: {DISABLED_MESSAGE}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn restore_external_tools_env(previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => std::env::set_var(EXTERNAL_TOOLS_ENV, value),
            None => std::env::remove_var(EXTERNAL_TOOLS_ENV),
        }
    }

    #[test]
    fn external_tools_are_disabled_by_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os(EXTERNAL_TOOLS_ENV);
        std::env::remove_var(EXTERNAL_TOOLS_ENV);

        assert!(!external_tools_enabled());
        assert!(require_external_tools_enabled("test command")
            .unwrap_err()
            .contains("OPENKOTO_ENABLE_EXTERNAL_TOOLS=1"));

        restore_external_tools_env(previous);
    }

    #[test]
    fn external_tools_can_be_enabled_by_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os(EXTERNAL_TOOLS_ENV);
        std::env::set_var(EXTERNAL_TOOLS_ENV, "1");

        assert!(external_tools_enabled());
        assert!(require_external_tools_enabled("test command").is_ok());

        restore_external_tools_env(previous);
    }
}
