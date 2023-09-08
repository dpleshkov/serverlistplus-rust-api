# ServerList+ Rust API

Rust implementation of the back-end API needed to provide the info for 
[ServerList+](https://starblast.dankdmitron.dev/). The back-end was originally written in 
Node.js but has been re-written in Rust to lower the memory footprint of the overall
server.

### Installation

You need the Rust toolchain installed alongside with Cargo. Building is
the same as with most other Rust projects:

```
git clone https://github.com/dpleshkov/serverlistplus-rust-api.git
cd serverlistplus-rust-api
cargo build
```

then run with

```
cargo run -- 3000 proxies_path.txt
```
where `3000` is the port and `proxies_path.txt` is an optional path to a text file containing an optional
list of HTTP proxies for the listeners to rotate between.

This will start the server with the same behavior as `https://starblast.dankdmitron.dev/api/`.
