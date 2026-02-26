use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const INSTALL_BIN_NAME: &str = "moire";
const INSTALL_BUNDLE_DIR_NAME: &str = "moire-web.dist";
const SOURCE_PACKAGE_NAME: &str = "moire-web";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("install") => install(),
        Some(command) => usage_and_exit(&format!("unknown command `{command}`")),
        None => usage_and_exit("missing command"),
    }
}

fn usage_and_exit(error: &str) -> ! {
    eprintln!("Error: {error}");
    eprintln!("Usage: cargo xtask <command>");
    eprintln!("Available commands:");
    eprintln!("  install");
    std::process::exit(1);
}

fn install() {
    if let Err(error) = install_inner() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn install_inner() -> Result<(), String> {
    let workspace_root = workspace_root()?;
    let frontend_dist = workspace_root.join("frontend/dist");
    let release_binary = workspace_root
        .join("target/release")
        .join(source_binary_name());
    let install_bin_dir = cargo_bin_dir()?;
    let install_binary_path = install_bin_dir.join(installed_binary_name());
    let install_bundle_path = install_bin_dir.join(INSTALL_BUNDLE_DIR_NAME);

    run(
        Command::new("pnpm")
            .arg("--filter")
            .arg("moire-frontend")
            .arg("build")
            .current_dir(&workspace_root),
        "build frontend with pnpm",
    )?;

    validate_frontend_dist(&frontend_dist)?;

    run(
        Command::new("cargo")
            .arg("build")
            .arg("--release")
            .arg("-p")
            .arg(SOURCE_PACKAGE_NAME)
            .current_dir(&workspace_root),
        "build moire-web release binary",
    )?;

    std::fs::create_dir_all(&install_bin_dir).map_err(|error| {
        format!(
            "failed to create install bin dir {}: {error}",
            install_bin_dir.display()
        )
    })?;

    std::fs::copy(&release_binary, &install_binary_path).map_err(|error| {
        format!(
            "failed to copy {} to {}: {error}",
            release_binary.display(),
            install_binary_path.display()
        )
    })?;
    println!("Installed binary to {}", install_binary_path.display());

    replace_dir(&frontend_dist, &install_bundle_path)?;
    println!(
        "Installed frontend bundle to {}",
        install_bundle_path.display()
    );

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("codesign")
            .arg("--sign")
            .arg("-")
            .arg("--force")
            .arg(&install_binary_path)
            .status()
            .map_err(|error| {
                format!(
                    "failed to run codesign for {}: {error}",
                    install_binary_path.display()
                )
            })?;
        if !status.success() {
            return Err(format!(
                "codesign failed for {} with status {status}",
                install_binary_path.display()
            ));
        }
    }

    run(
        Command::new(&install_binary_path).arg("--help"),
        "verify installed binary",
    )?;
    println!("Verified {}", install_binary_path.display());

    Ok(())
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(PathBuf::from)
        .ok_or_else(|| {
            format!(
                "failed to resolve workspace root from {}",
                manifest_dir.display()
            )
        })
}

fn validate_frontend_dist(frontend_dist: &Path) -> Result<(), String> {
    if !frontend_dist.is_dir() {
        return Err(format!(
            "frontend build output directory not found at {}",
            frontend_dist.display()
        ));
    }
    let index = frontend_dist.join("index.html");
    if !index.is_file() {
        return Err(format!(
            "frontend build output is missing {}",
            index.display()
        ));
    }
    Ok(())
}

fn run(command: &mut Command, what: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("failed to {what}: {error}"))?;
    if !status.success() {
        return Err(format!("{what} failed with status {status}"));
    }
    Ok(())
}

fn cargo_bin_dir() -> Result<PathBuf, String> {
    if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
        return Ok(PathBuf::from(cargo_home).join("bin"));
    }
    let home = std::env::var_os("HOME").ok_or("neither CARGO_HOME nor HOME is set")?;
    Ok(PathBuf::from(home).join(".cargo/bin"))
}

fn replace_dir(from: &Path, to: &Path) -> Result<(), String> {
    if to.exists() {
        std::fs::remove_dir_all(to).map_err(|error| {
            format!(
                "failed to remove existing directory {}: {error}",
                to.display()
            )
        })?;
    }
    copy_dir_recursive(from, to)
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to)
        .map_err(|error| format!("failed to create directory {}: {error}", to.display()))?;
    for entry in std::fs::read_dir(from)
        .map_err(|error| format!("failed to read directory {}: {error}", from.display()))?
    {
        let entry = entry.map_err(|error| {
            format!(
                "failed to read directory entry under {}: {error}",
                from.display()
            )
        })?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|error| format!("failed to read file type for {}: {error}", src.display()))?
            .is_dir()
        {
            copy_dir_recursive(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|error| {
                format!(
                    "failed to copy {} to {}: {error}",
                    src.display(),
                    dst.display()
                )
            })?;
        }
    }
    Ok(())
}

fn source_binary_name() -> OsString {
    if cfg!(windows) {
        OsString::from(format!("{SOURCE_PACKAGE_NAME}.exe"))
    } else {
        OsString::from(SOURCE_PACKAGE_NAME)
    }
}

fn installed_binary_name() -> OsString {
    if cfg!(windows) {
        OsString::from(format!("{INSTALL_BIN_NAME}.exe"))
    } else {
        OsString::from(INSTALL_BIN_NAME)
    }
}
