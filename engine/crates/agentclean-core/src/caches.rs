//! Global tool caches (F15, ADR 0006).
//!
//! A cache shared by every project (npm's, Go's module cache, pip's) is
//! cleared only by running that tool's own command with a fixed argument
//! list. The engine never deletes cache files itself and never quarantines
//! them: the tool owns the layout, and everything comes back by download.
//! Caches with no official command are measured and explained, nothing more.
//!
//! Toolchains (Homebrew's Cellar, nvm, rustup, global packages) are never a
//! cache: a cache path inside or above one is dropped (I20).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::size;

/// Where the caches of this user live. Built from the process environment by
/// [`Env::current`]; tests build it by hand.
#[derive(Debug, Clone)]
pub struct Env {
    pub home: PathBuf,
    pub macos: bool,
    /// `$XDG_CACHE_HOME` (Linux).
    pub xdg_cache: Option<PathBuf>,
    /// `$XDG_DATA_HOME` (Linux).
    pub xdg_data: Option<PathBuf>,
    pub npm_cache: Option<PathBuf>,
    pub gomodcache: Option<PathBuf>,
    pub gopath: Option<PathBuf>,
    pub cargo_home: Option<PathBuf>,
    /// Folders searched for the tools, in order.
    pub path: Vec<PathBuf>,
}

impl Env {
    pub fn current() -> Option<Self> {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        let home = var("HOME")?;
        let mut path: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        // An app started from Finder gets a bare PATH; add the usual places.
        // ponytail: nvm's npm is not found from Finder; add ~/.nvm/alias/default if asked.
        for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/local/go/bin"] {
            path.push(PathBuf::from(extra));
        }
        for extra in [
            ".cargo/bin",
            ".bun/bin",
            ".deno/bin",
            ".local/bin",
            ".volta/bin",
            "go/bin",
        ] {
            path.push(home.join(extra));
        }
        Some(Self {
            macos: cfg!(target_os = "macos"),
            xdg_cache: var("XDG_CACHE_HOME"),
            xdg_data: var("XDG_DATA_HOME"),
            npm_cache: var("npm_config_cache"),
            gomodcache: var("GOMODCACHE"),
            gopath: var("GOPATH"),
            cargo_home: var("CARGO_HOME"),
            home,
            path,
        })
    }

    /// The platform's per-user cache folder.
    fn cache(&self) -> PathBuf {
        if self.macos {
            self.home.join("Library/Caches")
        } else {
            self.xdg_cache
                .clone()
                .unwrap_or_else(|| self.home.join(".cache"))
        }
    }

    fn data(&self) -> PathBuf {
        self.xdg_data
            .clone()
            .unwrap_or_else(|| self.home.join(".local/share"))
    }

    fn cargo_home(&self) -> PathBuf {
        self.cargo_home
            .clone()
            .unwrap_or_else(|| self.home.join(".cargo"))
    }

    fn gopath(&self) -> PathBuf {
        self.gopath.clone().unwrap_or_else(|| self.home.join("go"))
    }

    /// Folders that hold installed tools, never caches.
    pub fn toolchains(&self) -> Vec<PathBuf> {
        let h = |p: &str| self.home.join(p);
        let mut t = vec![
            PathBuf::from("/opt/homebrew/Cellar"),
            PathBuf::from("/opt/homebrew/lib/node_modules"),
            PathBuf::from("/usr/local/Cellar"),
            PathBuf::from("/usr/local/lib/node_modules"),
            PathBuf::from("/Applications"),
            PathBuf::from("/Library/Developer"),
            h(".nvm"),
            h(".rustup"),
            h(".pyenv"),
            h(".volta"),
            h(".asdf"),
            h(".sdkman"),
            h(".bun/bin"),
            h(".deno/bin"),
            h(".local/share/mise"),
            h(".local/bin"),
            h("Library/pnpm/global"),
            self.cargo_home().join("bin"),
            self.gopath().join("bin"),
            self.data().join("pnpm/global"),
        ];
        if self.macos {
            t.push(h("Library/Developer/Xcode/iOS DeviceSupport"));
        }
        t
    }
}

/// One known cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    pub id: &'static str,
    pub tool: &'static str,
    pub name: &'static str,
    pub path: PathBuf,
    /// The tool's own command, program first; empty when there is none and
    /// the cache is only measured.
    pub command: &'static [&'static str],
    pub note: &'static str,
}

/// Every cache AgentClean knows on this platform, whether present or not.
/// I20: none is inside or above a toolchain folder, or the home folder.
pub fn known(env: &Env) -> Vec<Cache> {
    let c = env.cache();
    let mac = env.macos;
    let mut all = vec![
        Cache {
            id: "npm",
            tool: "npm",
            name: "npm package cache",
            path: env
                .npm_cache
                .clone()
                .unwrap_or_else(|| env.home.join(".npm"))
                .join("_cacache"),
            command: &["npm", "cache", "clean", "--force"],
            note: "Packages download again on the next install.",
        },
        Cache {
            id: "pnpm",
            tool: "pnpm",
            name: "pnpm store",
            path: if mac {
                env.home.join("Library/pnpm/store")
            } else {
                env.data().join("pnpm/store")
            },
            command: &["pnpm", "store", "prune"],
            note: "Removes only packages that no project uses.",
        },
        Cache {
            id: "yarn",
            tool: "yarn",
            name: "Yarn cache",
            path: c.join(if mac { "Yarn" } else { "yarn" }),
            command: &["yarn", "cache", "clean"],
            note: "Packages download again on the next install.",
        },
        Cache {
            id: "bun",
            tool: "bun",
            name: "Bun install cache",
            path: env.home.join(".bun/install/cache"),
            command: &["bun", "pm", "cache", "rm"],
            note: "Packages download again on the next install.",
        },
        Cache {
            id: "pip",
            tool: "pip",
            name: "pip cache",
            path: c.join("pip"),
            command: &["python3", "-m", "pip", "cache", "purge"],
            note: "Wheels download or build again on the next install.",
        },
        Cache {
            id: "uv",
            tool: "uv",
            name: "uv cache",
            path: env
                .xdg_cache
                .clone()
                .unwrap_or_else(|| env.home.join(".cache"))
                .join("uv"),
            command: &["uv", "cache", "clean"],
            note: "Packages download again on the next install.",
        },
        Cache {
            id: "go-mod",
            tool: "go",
            name: "Go module cache",
            path: env
                .gomodcache
                .clone()
                .unwrap_or_else(|| env.gopath().join("pkg/mod")),
            command: &["go", "clean", "-modcache"],
            note: "Modules download again on the next build.",
        },
        Cache {
            id: "go-build",
            tool: "go",
            name: "Go build cache",
            path: c.join("go-build"),
            command: &["go", "clean", "-cache"],
            note: "The next build compiles from scratch.",
        },
        Cache {
            id: "deno",
            tool: "deno",
            name: "Deno cache",
            path: c.join("deno"),
            command: &["deno", "clean"],
            note: "Modules download again on the next run.",
        },
        Cache {
            id: "composer",
            tool: "composer",
            name: "Composer cache",
            path: c.join("composer"),
            command: &["composer", "clear-cache"],
            note: "Packages download again on the next install.",
        },
        Cache {
            id: "cargo-registry",
            tool: "cargo",
            name: "Cargo registry",
            path: env.cargo_home().join("registry"),
            command: &[],
            note: "Cargo has no stable command to clear it.",
        },
        Cache {
            id: "gradle",
            tool: "gradle",
            name: "Gradle caches",
            path: env.home.join(".gradle/caches"),
            command: &[],
            note: "Gradle removes entries it has not used for 30 days by itself.",
        },
    ];
    if mac {
        all.extend([
            Cache {
                id: "cocoapods",
                tool: "pod",
                name: "CocoaPods cache",
                path: c.join("CocoaPods"),
                command: &["pod", "cache", "clean", "--all"],
                note: "Pods download again on the next install.",
            },
            Cache {
                id: "xcode-derived-data",
                tool: "xcode",
                name: "Xcode DerivedData",
                path: env.home.join("Library/Developer/Xcode/DerivedData"),
                command: &[],
                note: "Build output of every Xcode project. Xcode has no command for it; clean each project from its folder.",
            },
            Cache {
                id: "xcode-archives",
                tool: "xcode",
                name: "Xcode archives",
                path: env.home.join("Library/Developer/Xcode/Archives"),
                command: &[],
                note: "Archived builds and their debug symbols. Crash reports from a shipped version need its archive, so delete old ones yourself in Xcode's Organizer.",
            },
            Cache {
                id: "simulator-devices",
                tool: "xcode",
                name: "Simulators",
                path: env.home.join("Library/Developer/CoreSimulator/Devices"),
                command: &[],
                note: "Every simulator and the apps installed in it. xcrun simctl delete unavailable removes simulators for iOS versions no longer installed.",
            },
            Cache {
                id: "simulator-caches",
                tool: "xcode",
                name: "Simulator caches",
                path: env.home.join("Library/Developer/CoreSimulator/Caches"),
                command: &[],
                note: "Shared caches the simulators rebuild when they next start.",
            },
            Cache {
                id: "xcode-previews",
                tool: "xcode",
                name: "SwiftUI preview simulators",
                path: env.home.join("Library/Developer/Xcode/UserData/Previews"),
                command: &[],
                note: "Simulators Xcode makes for SwiftUI previews and makes again when needed.",
            },
            Cache {
                id: "xcode-cache",
                tool: "xcode",
                name: "Xcode cache",
                path: c.join("com.apple.dt.Xcode"),
                command: &[],
                note: "Downloads and indexes Xcode keeps for itself and rebuilds.",
            },
            Cache {
                id: "homebrew-downloads",
                tool: "brew",
                name: "Homebrew downloads",
                path: c.join("Homebrew"),
                command: &[],
                note: "brew cleanup also removes old versions of installed formulae, so AgentClean does not run it.",
            },
        ]);
    }
    let bad = env.toolchains();
    all.retain(|cache| allowed(&cache.path, &env.home, &bad));
    all
}

/// I20: a cache path must be absolute, below home or a system cache, and
/// neither inside nor above a toolchain folder or the home folder itself.
pub fn allowed(path: &Path, home: &Path, toolchains: &[PathBuf]) -> bool {
    path.is_absolute()
        && path != home
        && !home.starts_with(path)
        && path.components().count() > 2
        && !toolchains
            .iter()
            .any(|t| path.starts_with(t) || t.starts_with(path))
}

/// A cache that exists on disk, measured.
#[derive(Debug, Clone)]
pub struct Found {
    pub cache: Cache,
    pub bytes: u64,
    pub unreadable: u64,
    /// The tool's program, if the cache has a command and the tool is installed.
    pub program: Option<PathBuf>,
}

/// Caches present on disk (real folders, not links), measured.
pub fn list(env: &Env) -> Vec<Found> {
    known(env)
        .into_iter()
        .filter(|c| {
            std::fs::symlink_metadata(&c.path)
                .map(|m| m.is_dir())
                .unwrap_or(false)
        })
        .map(|cache| {
            let usage = size::usage(&cache.path);
            let program = cache.command.first().and_then(|p| which(p, &env.path));
            Found {
                bytes: usage.bytes,
                unreadable: usage.unreadable,
                program,
                cache,
            }
        })
        .collect()
}

/// The first executable regular file named `name` in `dirs`.
pub fn which(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| executable(p))
}

#[cfg(unix)]
fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn executable(p: &Path) -> bool {
    p.is_file()
}

/// How long a tool may take before it is stopped.
pub const TIME_LIMIT: Duration = Duration::from_secs(15 * 60);

/// What running a tool's command did.
#[derive(Debug, Clone)]
pub struct Ran {
    /// Exit status; `None` if it was stopped or killed by a signal.
    pub status: Option<i32>,
    /// The end of what it wrote to stderr.
    pub stderr: String,
}

/// Runs the cache's own command: fixed arguments, no shell, stdin closed,
/// working folder the home folder (never a project), the tool's own folder
/// first on PATH so scripts like npm find their interpreter.
pub fn run(found: &Found, env: &Env, limit: Duration) -> Result<Ran, String> {
    let program = found
        .program
        .as_ref()
        .ok_or_else(|| format!("{} is not installed", found.cache.tool))?;
    let mut path = Vec::new();
    if let Some(dir) = program.parent() {
        path.push(dir.to_path_buf());
    }
    path.extend(env.path.iter().cloned());
    // Tool scripts need the basics even when the engine got a bare PATH.
    path.extend(["/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from));
    let path = std::env::join_paths(path).map_err(|e| e.to_string())?;
    let mut child = Command::new(program)
        .args(&found.cache.command[1..])
        .current_dir(&env.home)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", program.display()))?;
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.code(),
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let stderr = reader.join().unwrap_or_default();
    let tail: String = stderr
        .lines()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Ran {
        status,
        stderr: tail,
    })
}
