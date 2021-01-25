# A Tool Box, Rust (ATB) 

A collection of tools used for rust based backend projects.  

# Usage

```Cargo.toml
[dependencies]
atb = { version = "0.6.5", features = ["http"] }
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

