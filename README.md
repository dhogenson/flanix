# Flanix

Flanix is a syncing program made in rust. I took inspiration from how syncthing could sync folders of your choice, but making it sync to the cloud instead of making it node based. And one other thing I plan to solve is to make this run on phones to (android can but it does not have a app) 

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
