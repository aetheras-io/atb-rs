# A Tool Box, Rust (ATB)

Small, focused crates shared by Aetheras Rust backend projects.

## Crates.io packages

The reusable components are released independently under the `atb-*` crate names:

- `atb-types`
- `atb-tokio-ext`
- `atb-serde-ext`
- `atb-graphql-ext` and `atb-graphql-derive`
- `atb-ai-utils`
- `atb-build-utils`
- `atb-cli-utils`
- `atb-fixtures-utils`
- `atb-logging`

Add only the components a project uses:

```toml
[dependencies]
atb-types = { version = "2", features = ["jwt"] }
atb-tokio-ext = "2"
```

The root `atb` facade remains available from Git because the bare `atb` name on crates.io belongs to
another project and its optional event-sourcing integration currently uses Git dependencies:

```toml
[dependencies]
atb = { git = "https://github.com/aetheras-io/atb-rs", tag = "v2.1.0", features = ["eventsourcing", "fixtures"] }
```

## Temporal

ATB's former SDK Core compatibility macros were retired in 2.0. Use the official native
`temporalio-*` 0.7 crates and their typed activity and workflow APIs directly.

## License

Licensed under either the Apache License, Version 2.0 or the MIT license, at your option.
