pub mod activities;

pub use atb_temporal_derive::*;

//#HACK for macro expansion tests
#[cfg(test)]
extern crate self as atb_temporal_ext;

#[cfg(test)]
mod tests {
    use super::*;
    use temporalio_sdk::{ActExitValue, ActivityError};

    #[activity]
    async fn remote_only_ctx(_ctx: ActContext) -> Result<ActExitValue<()>, ActivityError> {
        Ok(().into())
    }

    #[local_activity]
    async fn local_with_req(
        _ctx: ActContext,
        req: u32,
    ) -> Result<ActExitValue<u32>, ActivityError> {
        Ok((req + 1).into())
    }

    #[test]
    fn macro_paths_compile() {
        // Nothing to run; the test passes if the module compiled.
    }
}
