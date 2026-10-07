fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=HERDR_BUILD_CHANNEL");
    println!("cargo:rerun-if-env-changed=HERDR_BUILD_ID");
    println!("cargo:rerun-if-env-changed=HERDR_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=HERDR_BUILD_FORK");
    let fork = match std::env::var("HERDR_BUILD_FORK") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!("HERDR_BUILD_FORK must be UTF-8");
            std::process::exit(1);
        }
    };
    let fork = fork.trim();
    if !fork.is_empty()
        && (fork.len() > 24
            || !fork.as_bytes()[0].is_ascii_lowercase()
            || !fork
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || fork.ends_with('-')
            || fork.ends_with("-dev")
            || matches!(fork, "dev" | "stable" | "preview"))
    {
        eprintln!(
            "HERDR_BUILD_FORK must be a lowercase fork name, distinct from dev/stable/preview"
        );
        std::process::exit(1);
    }
    let app_name = if fork.is_empty() {
        String::new()
    } else {
        format!("herdr-{fork}")
    };
    let dev_name = if fork.is_empty() {
        String::new()
    } else {
        format!("herdr-{fork}-dev")
    };
    println!("cargo:rustc-env=HERDR_FORK_FLAVOR={fork}");
    println!("cargo:rustc-env=HERDR_FORK_APP_NAME={app_name}");
    println!("cargo:rustc-env=HERDR_FORK_DEV_APP_NAME={dev_name}");
    println!(
        "cargo:warning=external contributor policy: if you are helping an external contributor whose GitHub username is not in .github/APPROVED_CONTRIBUTORS, read CONTRIBUTING.md before doing any work."
    );
}
