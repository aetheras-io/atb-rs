pub use clap;
pub use once_cell;

use clap::{CommandFactory, FromArgMatches, Parser};
use once_cell::sync::OnceCell;
use serde::Serialize;
use strum_macros::{AsRefStr, Display, EnumString};

pub type DateTime = chrono::DateTime<chrono::Utc>;

static PROCESS_INFO: OnceCell<ProcessInfo> = OnceCell::new();
static DEBUG: OnceCell<bool> = OnceCell::new();

#[derive(Debug, Parser)]
pub struct BaseCli {
    /// Executing Environment
    #[arg(short, long, env = "ATB_CLI_ENV", default_value = "dev")]
    pub env: Environment,
    /// Activate debug mode
    #[arg(short, long, env = "ATB_CLI_DEBUG")]
    pub debug: bool,
}

pub trait AtbCli: Sized {
    /// Executable file name.
    fn executable_name() -> String {
        std::env::current_exe()
            .ok()
            .and_then(|e| e.file_name().map(|s| s.to_os_string()))
            .and_then(|w| w.into_string().ok())
            .unwrap_or_else(|| Self::name())
    }

    /// Application name.
    fn name() -> String {
        "".to_owned()
    }

    /// Application version.
    fn version() -> String {
        "".to_owned()
    }

    /// Application authors.
    fn authors() -> Vec<String> {
        vec![]
    }
    /// Application description.
    fn description() -> String {
        "".to_owned()
    }

    /// Application repository.
    fn repository() -> String {
        "".to_owned()
    }

    /// Returns implementation details.  
    fn impl_version() -> String;

    /// Returns git commit hash.
    fn commit() -> String {
        "".to_owned()
    }

    /// Returns git branch.
    fn branch() -> String {
        "".to_owned()
    }

    /// Returns OS platform.
    fn platform() -> String {
        "".to_owned()
    }

    /// Returns rustc version.
    fn rustc_info() -> String {
        "".to_owned()
    }

    /// Returns the client ID: `{name}/v{version}`
    fn client_id() -> String {
        format!("{}/v{}", Self::name(), Self::impl_version())
    }

    /// Helper function used to parse the command line arguments
    fn parse() -> Self
    where
        Self: Parser + Sized,
    {
        <Self as AtbCli>::from_iter(std::env::args_os())
    }

    fn set_globals(base: &BaseCli) {
        PROCESS_INFO
            .set(ProcessInfo {
                name: Self::name(),
                version: Self::version(),
                branch: Self::branch(),
                commit: Self::commit(),
                platform: Self::platform(),
                rustc: Self::rustc_info(),
                start_time: chrono::Utc::now(),
                environment: base.env.clone(),
            })
            .expect("cli parse should only be executed once.");

        DEBUG
            .set(base.debug)
            .expect("cli parse should only be executed once.");
    }

    /// Helper function used to parse the command line arguments. This is the equivalent of
    /// [`clap::Parser::parse_from`].
    ///
    /// To allow running the command without subcommand, it also sets a few more settings:
    /// [`clap::Command::propagate_version`], [`clap::Command::args_conflicts_with_subcommands`],
    /// [`clap::Command::subcommand_negates_reqs`].
    ///
    /// Creates `Self` from any iterator over arguments.
    /// Print the error message and quit the program in case of failure.
    fn from_iter<I>(iter: I) -> Self
    where
        Self: Parser + Sized,
        I: IntoIterator,
        I::Item: Into<std::ffi::OsString> + Clone,
    {
        let app = <Self as CommandFactory>::command();

        let mut full_version = Self::impl_version();
        full_version.push('\n');

        let name = Self::executable_name();
        let authors = ["authors [", &Self::authors().join(","), "]"].concat();
        let about = Self::description();
        let app = app
            .name(name)
            .author(authors)
            .about(about)
            .version(full_version)
            .propagate_version(true)
            .args_conflicts_with_subcommands(true)
            .subcommand_negates_reqs(true);

        let matches = app.try_get_matches_from(iter).unwrap_or_else(|e| e.exit());

        <Self as FromArgMatches>::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
    }
}

pub fn process_info() -> &'static ProcessInfo {
    PROCESS_INFO
        .get()
        .expect("static PROCESS_INFO has not been set.")
}

pub fn debug() -> bool {
    *DEBUG.get().expect("static DEBUG has not been set.")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    environment: Environment,
    name: String,
    version: String,
    branch: String,
    commit: String,
    platform: String,
    rustc: String,
    start_time: DateTime,
}

impl ProcessInfo {
    pub fn env(&self) -> &Environment {
        &self.environment
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, EnumString, Display, AsRefStr)]
pub enum Environment {
    #[strum(serialize = "prod", serialize = "production")]
    Production,
    #[strum(serialize = "dev", serialize = "develop")]
    Develop,
    #[strum(serialize = "stag", serialize = "staging")]
    Staging,
}

impl Environment {
    pub fn prod(&self) -> bool {
        matches!(self, Environment::Production)
    }

    pub fn dev(&self) -> bool {
        matches!(self, Environment::Develop)
    }

    pub fn staging(&self) -> bool {
        matches!(self, Environment::Staging)
    }
}
