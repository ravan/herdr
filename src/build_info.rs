//! Build identity helpers.

pub const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn channel() -> &'static str {
    fork_name()
        .or_else(|| non_empty(option_env!("HERDR_BUILD_CHANNEL")))
        .unwrap_or("stable")
}

pub fn fork_name() -> Option<&'static str> {
    non_empty(option_env!("HERDR_FORK_FLAVOR"))
}

pub fn executable_name() -> &'static str {
    non_empty(option_env!("HERDR_FORK_APP_NAME")).unwrap_or("herdr")
}

pub fn fork_app_dir_name() -> Option<&'static str> {
    if cfg!(debug_assertions) {
        non_empty(option_env!("HERDR_FORK_DEV_APP_NAME"))
    } else {
        non_empty(option_env!("HERDR_FORK_APP_NAME"))
    }
}

pub fn build_id() -> Option<&'static str> {
    non_empty(option_env!("HERDR_BUILD_ID"))
}

pub fn version() -> String {
    match channel() {
        "stable" => BASE_VERSION.to_string(),
        channel => match build_id() {
            Some(build_id) => format!("{BASE_VERSION}-{channel}.{build_id}"),
            None => format!("{BASE_VERSION}-{channel}"),
        },
    }
}

pub fn is_preview() -> bool {
    channel() == "preview"
}

fn non_empty(value: Option<&'static str>) -> Option<&'static str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn fork_build_identity_keeps_version_and_runtime_namespace_separate() {
        if option_env!("HERDR_BUILD_FORK") == Some("houston") {
            assert_eq!(super::channel(), "houston");
            assert!(super::version().starts_with(&format!("{}-houston", super::BASE_VERSION)));
            assert_eq!(
                crate::config::app_dir_name(),
                if cfg!(debug_assertions) {
                    "herdr-houston-dev"
                } else {
                    "herdr-houston"
                }
            );
        } else if option_env!("HERDR_BUILD_FORK").is_none() {
            assert_eq!(
                crate::config::app_dir_name(),
                if cfg!(debug_assertions) {
                    "herdr-dev"
                } else {
                    "herdr"
                }
            );
        }
    }

    #[test]
    fn stable_version_defaults_to_cargo_version() {
        assert!(!super::version().is_empty());
    }
}
