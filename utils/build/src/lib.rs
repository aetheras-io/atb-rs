use platforms::*;
use std::{borrow::Cow, env, fs, fs::File, io, io::Read, path::PathBuf, process::Command};

/// Generate the `cargo:` key output
pub fn generate_cargo_keys() {
    let commit = if let Ok(hash) = std::env::var("ATB_CLI_GIT_COMMIT_HASH") {
        Cow::from(hash.trim().to_owned())
    } else {
        match Command::new("git")
            .args(&["rev-parse", "--short", "HEAD"])
            .output()
        {
            Ok(o) if o.status.success() => {
                let sha = String::from_utf8_lossy(&o.stdout).trim().to_owned();
                Cow::from(sha)
            }
            Ok(o) => {
                println!("cargo:warning=Git command failed with status: {}", o.status);
                Cow::from("unknown")
            }
            Err(err) => {
                println!("cargo:warning=Failed to execute git command: {}", err);
                Cow::from("unknown")
            }
        }
    };

    println!(
        "cargo:rustc-env=ATB_CLI_IMPL_VERSION={}",
        get_version(&commit)
    )
}

fn get_platform() -> String {
    let env_dash = if TARGET_ENV.is_some() { "-" } else { "" };

    format!(
        "{}-{}{}{}",
        TARGET_ARCH.as_str(),
        TARGET_OS.as_str(),
        env_dash,
        TARGET_ENV.map(|x| x.as_str()).unwrap_or(""),
    )
}

fn get_version(impl_commit: &str) -> String {
    let commit_dash = if impl_commit.is_empty() { "" } else { "-" };

    format!(
        "{}{}{}-{}",
        std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
        commit_dash,
        impl_commit,
        get_platform(),
    )
}

/// Make sure the calling `build.rs` script is rerun when `.git/HEAD` or the ref of `.git/HEAD`
/// changed.
///
/// The file is searched from the `CARGO_MANIFEST_DIR` upwards. If the file can not be found,
/// a warning is generated.
pub fn rerun_if_git_head_changed() {
    let mut manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("`CARGO_MANIFEST_DIR` is always set by cargo."),
    );
    let manifest_dir_copy = manifest_dir.clone();

    while manifest_dir.parent().is_some() {
        match get_git_paths(&manifest_dir) {
            Err(err) => {
                eprintln!("cargo:warning=Unable to read the Git repository: {}", err);

                return;
            }
            Ok(None) => {}
            Ok(Some(paths)) => {
                for p in paths {
                    println!("cargo:rerun-if-changed={}", p.display());
                }

                return;
            }
        }

        manifest_dir.pop();
    }

    println!(
        "cargo:warning=Could not find `.git/HEAD` searching from `{}` upwards!",
        manifest_dir_copy.display(),
    );
}

// Code taken from https://github.com/rustyhorde/vergen/blob/8d522db8c8e16e26c0fc9ea8e6b0247cbf5cca84/src/output/envvar.rs
fn get_git_paths(path: &PathBuf) -> Result<Option<Vec<PathBuf>>, io::Error> {
    let git_dir_or_file = path.join(".git");

    if let Ok(metadata) = fs::metadata(&git_dir_or_file) {
        if metadata.is_dir() {
            // Echo the HEAD path
            let git_head_path = git_dir_or_file.join("HEAD");

            // Determine where HEAD points and echo that path also.
            let mut f = File::open(&git_head_path)?;
            let mut git_head_contents = String::new();
            let _ = f.read_to_string(&mut git_head_contents)?;
            let ref_vec: Vec<&str> = git_head_contents.split(": ").collect();

            if ref_vec.len() == 2 {
                let current_head_file = ref_vec[1];
                let git_refs_path = git_dir_or_file.join(current_head_file);

                Ok(Some(vec![git_head_path, git_refs_path]))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "You are most likely in a detached HEAD state",
                ))
            }
        } else if metadata.is_file() {
            // We are in a worktree, so find out where the actual worktrees/<name>/HEAD file is.
            let mut git_file = File::open(&git_dir_or_file)?;
            let mut git_contents = String::new();
            let _ = git_file.read_to_string(&mut git_contents)?;
            let dir_vec: Vec<&str> = git_contents.split(": ").collect();
            let git_path = dir_vec[1].trim();

            // Echo the HEAD psth
            let git_head_path = PathBuf::from(git_path).join("HEAD");

            // Find out what the full path to the .git dir is.
            let mut actual_git_dir = PathBuf::from(git_path);
            actual_git_dir.pop();
            actual_git_dir.pop();

            // Determine where HEAD points and echo that path also.
            let mut f = File::open(&git_head_path)?;
            let mut git_head_contents = String::new();
            let _ = f.read_to_string(&mut git_head_contents)?;
            let ref_vec: Vec<&str> = git_head_contents.split(": ").collect();

            if ref_vec.len() == 2 {
                let current_head_file = ref_vec[1];
                let git_refs_path = actual_git_dir.join(current_head_file);

                Ok(Some(vec![git_head_path, git_refs_path]))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "You are most likely in a detached HEAD state",
                ))
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Invalid .git format (Not a directory or a file)",
            ))
        }
    } else {
        Ok(None)
    }
}
