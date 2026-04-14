use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
Usage:
  rzn-python-tools status [--json]
  rzn-python-tools paths [--json]
  rzn-python-tools workflows sync [--dest PATH] [--force]
  rzn-python-tools worker [-- <args passed to rzn-python-worker>]
";

#[derive(Debug, Deserialize, Serialize)]
struct InstallManifest {
    schema_version: u32,
    artifact: String,
    plugin_variant_id: String,
    variant: String,
    version: String,
    cargo_version: String,
    platform: String,
    description: String,
    paths: ManifestPaths,
    bundled_python: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct ManifestPaths {
    cli: String,
    worker: String,
    examples: String,
    workflows: String,
    system_metadata: String,
}

#[derive(Debug, Serialize)]
struct StatusOutput {
    artifact: String,
    version: String,
    variant: String,
    platform: String,
    install_root: String,
    worker_path: String,
    bundled_python: bool,
    workflows_source: String,
    default_workflows_dest: String,
}

#[derive(Debug, Serialize)]
struct PathsOutput {
    install_root: String,
    cli_path: String,
    worker_path: String,
    examples_path: String,
    workflows_source: String,
    system_metadata_path: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "status" => cmd_status(&args[2..]),
        "paths" => cmd_paths(&args[2..]),
        "workflows" => cmd_workflows(&args[2..]),
        "worker" | "serve" => cmd_worker(&args[2..]),
        "-h" | "--help" | "" => {
            print!("{USAGE}");
            Ok(())
        }
        _ => Err(anyhow::anyhow!("unknown command: {}\n\n{}", cmd, USAGE)),
    }
}

fn install_root() -> anyhow::Result<PathBuf> {
    if let Ok(root) = env::var("RZN_PYTHON_TOOLS_HOME") {
        let trimmed = root.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let exe = env::current_exe()?;
    let canonical = fs::canonicalize(exe)?;
    let parent = canonical
        .parent()
        .ok_or_else(|| anyhow::anyhow!("failed to resolve binary directory"))?;
    let install_root = parent
        .parent()
        .ok_or_else(|| anyhow::anyhow!("failed to resolve install root"))?;
    Ok(install_root.to_path_buf())
}

fn read_manifest(root: &Path) -> anyhow::Result<InstallManifest> {
    let raw = fs::read_to_string(root.join("manifest.json"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn default_workflows_dest() -> PathBuf {
    if let Ok(v) = env::var("RZN_PYTHON_TOOLS_WORKFLOWS_DIR") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home)
            .join(".rzn")
            .join("python-tools")
            .join("workflows");
    }
    env::temp_dir()
        .join("rzn")
        .join("python-tools")
        .join("workflows")
}

fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn wants_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

fn cmd_status(args: &[String]) -> anyhow::Result<()> {
    let root = install_root()?;
    let manifest = read_manifest(&root)?;
    let output = StatusOutput {
        artifact: manifest.artifact,
        version: manifest.version,
        variant: manifest.variant,
        platform: manifest.platform,
        install_root: root.display().to_string(),
        worker_path: root.join(&manifest.paths.worker).display().to_string(),
        bundled_python: manifest.bundled_python,
        workflows_source: root.join(&manifest.paths.workflows).display().to_string(),
        default_workflows_dest: default_workflows_dest().display().to_string(),
    };

    if wants_json(args) {
        return print_json(&output);
    }

    println!("artifact: {}", output.artifact);
    println!("version: {}", output.version);
    println!("variant: {}", output.variant);
    println!("platform: {}", output.platform);
    println!("install_root: {}", output.install_root);
    println!("worker: {}", output.worker_path);
    println!("bundled_python: {}", output.bundled_python);
    println!("workflows_source: {}", output.workflows_source);
    println!("default_workflows_dest: {}", output.default_workflows_dest);
    Ok(())
}

fn cmd_paths(args: &[String]) -> anyhow::Result<()> {
    let root = install_root()?;
    let manifest = read_manifest(&root)?;
    let cli_path = fs::canonicalize(env::current_exe()?)?;
    let output = PathsOutput {
        install_root: root.display().to_string(),
        cli_path: cli_path.display().to_string(),
        worker_path: root.join(&manifest.paths.worker).display().to_string(),
        examples_path: root.join(&manifest.paths.examples).display().to_string(),
        workflows_source: root.join(&manifest.paths.workflows).display().to_string(),
        system_metadata_path: root
            .join(&manifest.paths.system_metadata)
            .display()
            .to_string(),
    };

    if wants_json(args) {
        return print_json(&output);
    }

    println!("install_root: {}", output.install_root);
    println!("cli: {}", output.cli_path);
    println!("worker: {}", output.worker_path);
    println!("examples: {}", output.examples_path);
    println!("workflows_source: {}", output.workflows_source);
    println!("system_metadata: {}", output.system_metadata_path);
    Ok(())
}

fn cmd_workflows(args: &[String]) -> anyhow::Result<()> {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("");
    match subcmd {
        "sync" => cmd_workflows_sync(&args[1..]),
        _ => Err(anyhow::anyhow!(
            "unknown workflows command: {}\n\n{}",
            subcmd,
            USAGE
        )),
    }
}

fn cmd_workflows_sync(args: &[String]) -> anyhow::Result<()> {
    let root = install_root()?;
    let manifest = read_manifest(&root)?;
    let mut dest = default_workflows_dest();
    let mut force = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dest" => {
                let value = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow::anyhow!("missing value for --dest"))?;
                dest = PathBuf::from(value);
                i += 2;
            }
            "--force" => {
                force = true;
                i += 1;
            }
            other => {
                return Err(anyhow::anyhow!("unknown workflows sync arg: {other}"));
            }
        }
    }

    let src = root.join(&manifest.paths.examples);
    if !src.exists() {
        return Err(anyhow::anyhow!(
            "missing bundled examples at {}",
            src.display()
        ));
    }
    if force && dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    copy_dir_recursive(&src, &dest)?;
    println!("synced workflows to {}", dest.display());
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

fn cmd_worker(args: &[String]) -> anyhow::Result<()> {
    let root = install_root()?;
    let manifest = read_manifest(&root)?;
    let worker = root.join(&manifest.paths.worker);
    let mut cmd = Command::new(&worker);
    cmd.args(strip_separator(args));
    cmd.env("RZN_PLUGIN_DIR", &root);
    let status = cmd.status()?;
    if status.success() {
        return Ok(());
    }
    match status.code() {
        Some(code) => Err(anyhow::anyhow!("worker exited with status {}", code)),
        None => Err(anyhow::anyhow!("worker terminated by signal")),
    }
}

fn strip_separator(args: &[String]) -> Vec<String> {
    if args.first().map(|s| s.as_str()) == Some("--") {
        return args[1..].to_vec();
    }
    args.to_vec()
}
