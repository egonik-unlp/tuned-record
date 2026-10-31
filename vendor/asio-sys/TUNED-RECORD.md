# Vendored asio-sys 0.4.0

Copy of `asio-sys` 0.4.0 (Apache-2.0, from crates.io, the version cpal 0.18.2 uses), wired in
through `[patch.crates-io]` in the top-level `Cargo.toml`. Only change: `build.rs` marks
`AsioDrivers` and `AsioDriverList` as opaque types for bindgen.

Why: under the Itanium C++ ABI that MinGW uses (`x86_64-pc-windows-gnu`), `AsioDrivers`' own
fields are packed into the tail padding of its non-POD base class `AsioDriverList`. A Rust
`_base` field can't express that, so the generated layout assertions fail to compile. MSVC does
not reuse tail padding, which is why upstream never sees it. Rust code never touches those
fields (only the SDK's C++ does), so opaque blobs of the right size and alignment are exact.

When upgrading cpal: check whether the new asio-sys still needs this; if so, re-apply the two
`.opaque_type(...)` lines to its `build.rs`.
