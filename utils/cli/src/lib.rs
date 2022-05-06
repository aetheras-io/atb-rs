pub use clap;

use clap::{CommandFactory, FromArgMatches, Parser};

pub trait AtbCli: Sized {
    /// Executable file name.
    fn executable_name() -> String {
        std::env::current_exe()
            .ok()
            .and_then(|e| e.file_name().map(|s| s.to_os_string()))
            .and_then(|w| w.into_string().ok())
            .unwrap_or_else(|| Self::name())
    }

    /// Cargo crate name.
    fn name() -> String {
        env!("CARGO_PKG_NAME").to_owned()
    }

    /// Cargo crate version.
    fn version() -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// Cargo crate description.
    fn description() -> String {
        env!("CARGO_PKG_DESCRIPTION").to_owned()
    }

    /// Cargo crate author.
    fn author() -> String {
        env!("CARGO_PKG_AUTHORS").to_owned()
    }

    /// Cargo crate repository url
    fn repository_url() -> String {
        env!("CARGO_PKG_REPOSITORY").to_owned()
    }

    /// Returns the client ID: `{name}/v{version}`
    fn client_id() -> String {
        format!("{}/v{}", Self::name(), Self::version())
    }

    /// Returns git commit hash.  Requires build time helpers using atb-rs/utils/build
    fn commit_hash() -> String {
        std::env::var("ATB_CLI_GIT_COMMIT_HASH").unwrap_or_else(|_| "".to_owned())
    }

    /// Returns implementation.  Requires build time helpers using atb-rs/utils/build
    fn impl_version() -> String {
        std::env::var("ATB_CLI_IMPL_VERSION").unwrap_or_else(|_| "".to_owned())
    }

    /// Helper function used to parse the command line arguments
    fn from_args() -> Self
    where
        Self: Parser + Sized,
    {
        <Self as AtbCli>::from_iter(&mut std::env::args_os())
    }

    /// Helper function used to parse the command line arguments. This is the equivalent of
    /// [`clap::Parser::parse_from`].
    ///
    /// To allow running the node without subcommand, it also sets a few more settings:
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

        let mut full_version = Self::version();
        full_version.push_str("\n");

        let name = Self::executable_name();
        let author = Self::author();
        let about = Self::description();
        let app = app
            .name(name)
            .author(author.as_str())
            .about(about.as_str())
            .version(full_version.as_str())
            .propagate_version(true)
            .args_conflicts_with_subcommands(true)
            .subcommand_negates_reqs(true);

        let matches = app.try_get_matches_from(iter).unwrap_or_else(|e| e.exit());

        <Self as FromArgMatches>::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
    }
}
