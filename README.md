# Overview

This is a syncing program made in Rust. It's like onedrive but I plain to make it run on Android and maybe iOS.

# Building

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

# Building for Termux (Android)

**NOTE:** You need to install the Android NDK as a linker

``` 
rustup target add aarch64-linux-android
cargo install cargo-ndk
cargo ndk -t arm64-v8a build --release
```
