# Aetheras Tool Box, Rust (ATB) 

A collection of standardized tools used for rust based backend projects.  

# Usage

```Cargo.toml
[dependencies]
atb = { git = "https://github.com/aetheras-io/atb-rs", tag = "v0.1.0", features = ["http"] }
```

```rs
use atb::prelude::*;
use actix_web;
use uuid;
...
```
## Available Features

- Http (Actix)
- Graphql (Juniper)
- Sql (Sqlx)
- JWT
- Fixtures

