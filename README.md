# Overview

This is a syncing program made in Rust. It's like onedrive but I plain to make it run on Android and maybe iOS.

# Building for Linux

If you are on Linux run this
```
cargo build --release
```

# Building for Windows on Linux

Install the msvc toolchain and install xwin and then build
```
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin
cargo xwin build --release --target x86_64-pc-windows-msvc
```
