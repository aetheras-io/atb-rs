mod activity;

use proc_macro::TokenStream;
use syn::{ItemFn, parse_macro_input};

/// Attribute macro for defining a Temporal **remote activity** using idiomatic Rust.
///
/// # Overview
/// This macro lets you write a normal Rust `async fn` returning a
/// `Result<temporalio_sdk::ActExitValue<T>, ActivityError>` (or any alias that expands to that),
/// and it automatically generates:
///
/// - a **builder struct** (`FooActivity<'a>`) used inside workflows  
/// - user code wrapped in an async **base** method (`FooActivity::base`) that retains your
///   original signature and body  
/// - a **handler** (`FooActivity::handler`) that calls `base` and surfaces the
///   `ActExitValue<T>` result to Temporal workers  
/// - a **bind()** helper to register the activity with a worker  
/// - a **workflow-facing constructor** that keeps your original function name (`foo()`)
/// - sensible defaults, including `start_to_close_timeout: Some(Duration::from_secs(10))`
///
/// # Example (user code)
///
/// ```rust
/// #[activity]
/// pub async fn generate_ids(
///     ctx: ActContext,
///     count: u32,
/// ) -> Result<ActExitValue<Vec<Uuid>>, ActivityError> {
///     Ok((0..count).map(|_| Uuid::new_v4()).collect::<Vec<_>>().into())
/// }
/// ```
///
/// # Exact expansion for the function above
///
/// ```rust
/// pub struct GenerateIdsActivity<'a> {
///     ctx: &'a temporalio_sdk::WfContext,
///     options: temporalio_sdk::ActivityOptions,
/// }
///
/// impl<'a> GenerateIdsActivity<'a> {
///     fn new(
///         ctx: &'a temporalio_sdk::WfContext,
///         request: &'a u32,
///     ) -> crate::temporal::activities::utils::ActivityResult<Self> {
///         use temporalio_sdk_core_protos::coresdk::AsJsonPayloadExt;
///         let input = request
///             .as_json_payload()
///             .map_err(crate::temporal::activities::utils::ActivityRunError::PayloadEncode)?;
///         Ok(Self {
///             ctx,
///             options: temporalio_sdk::ActivityOptions {
///                 activity_type: "generate_ids".to_string(),
///                 start_to_close_timeout: Some(std::time::Duration::from_secs(10)),
///                 input,
///                 ..temporalio_sdk::ActivityOptions::default()
///             },
///         })
///     }
///
///     pub fn with_options(mut self, options: temporalio_sdk::ActivityOptions) -> Self {
///         let base = self.options;
///         self.options = temporalio_sdk::ActivityOptions {
///             activity_type: base.activity_type,
///             input: base.input,
///             ..options
///         };
///        self
///     }
///
///     pub fn resolution(
///         self,
///     ) -> impl std::future::Future<
///         Output = temporalio_sdk_core_protos::coresdk::activity_result::ActivityResolution
///     > + 'a {
///         async move {
///             let ctx = self.ctx;
///             let options = self.options;
///             ctx.activity(options).await
///         }
///     }
///
///     pub fn run(
///         self,
///     ) -> impl std::future::Future<
///         Output = crate::temporal::activities::utils::ActivityResult<Vec<Uuid>>
///     > + 'a {
///         async move {
///             let resolution = self.resolution().await;
///             crate::temporal::activities::utils::resolution_value::<Vec<Uuid>>(resolution)
///         }
///     }
/// }
///
/// impl GenerateIdsActivity<'static> {
///     pub async fn handler(
///         ctx: temporalio_sdk::ActContext,
///         count: u32,
///     ) -> Result<temporalio_sdk::ActExitValue<Vec<Uuid>>, temporalio_sdk::ActivityError> {
///         Ok((0..count).map(|_| Uuid::new_v4()).collect::<Vec<_>>().into())
///     }
///
///     pub fn bind(worker: &mut temporalio_sdk::Worker) {
///         worker.register_activity("generate_ids", Self::handler);
///     }
/// }
///
/// pub fn generate_ids<'a>(
///     ctx: &'a temporalio_sdk::WfContext,
///     count: &'a u32,
/// ) -> crate::temporal::activities::utils::ActivityResult<GenerateIdsActivity<'a>> {
///     GenerateIdsActivity::new(ctx, count)
/// }
/// ```
///
/// The generated builder sets `start_to_close_timeout` to 10 seconds by default; override it with
/// `with_options(...)` if your activity needs more time.
///
/// Request serialization errors from `AsJsonPayloadExt` surface as
/// `ActivityRunError::PayloadEncode`, letting you handle them the same way you handle
/// resolution/response decode failures in workflow code.
///
/// The exact same scaffolding is produced for `#[local_activity2]` except that it stores
/// `temporalio_sdk::LocalActivityOptions`, calls `WfContext::local_activity`, and the rest of the
/// workflow helper API stays the same.
///
/// # Summary
///
/// - **Workers call:** `GenerateIdsActivity::handler`  
/// - **Workflows call:** `generate_ids(ctx, &count)?.run().await`  
/// - You write only the idiomatic Rust function — the macro builds the Temporal plumbing.
///
/// # Workflow usage
///
/// ```rust
/// use std::time::Duration;
/// use temporalio_sdk::{workflow, ActivityOptions, WfContext, WorkflowResult};
/// use uuid::Uuid;
///
/// #[workflow]
/// async fn onboarding(ctx: WfContext) -> WorkflowResult<()> {
///     let count = 10u32;
///
///    // Builder mirrors your function name and request parameters.
///    let _ids = my_activities::generate_ids(ctx, &count)?
///        .with_options(ActivityOptions {
///            schedule_to_close_timeout: Some(Duration::from_secs(30)),
///            // keep the generated 10s start_to_close_timeout unless you need longer
///            ..Default::default()
///        })
///        .run()
///        .await?;
///
///    // ...use ids in the workflow...
///    Ok(().into())
/// }
/// ```
///
/// # See also
/// - [`local_activity`] for local activities (identical API, different runtime behavior)
#[proc_macro_attribute]
pub fn activity(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    match activity::expand_activity(input_fn, activity::ActivityKind::Remote) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Attribute macro for defining a Temporal **local activity**.
///
/// It has the same ergonomic surface area as [`activity`], but targets
/// `temporalio_sdk::LocalActivityOptions` and `WfContext::local_activity` under the hood.
/// In other words, given an idiomatic `async fn` that returns `Result<T, ActivityError>`, the
/// macro generates:
///
/// - `FooActivity<'a>` builder (local options + payload serialization)
/// - `FooActivity::base` with your original function body
/// - `FooActivity::handler` + `bind()` for worker registration
/// - workflow helper `foo(ctx, &req)?` mirroring your function name
///
/// Use this when the work can safely run on the workflow worker process and you want lower
/// latency / no network hop; otherwise prefer [`activity`].
#[proc_macro_attribute]
pub fn local_activity(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    match activity::expand_activity(input_fn, activity::ActivityKind::Local) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
