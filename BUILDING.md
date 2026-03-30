# Building mint-notag

This guide should assist you in compiling and running mint-notag on both Windows and Linux.

## Requirements

### Windows

- Visual Studio with the following individual components:
  - MSVC v143 - VS 2022 C++ x64/x86 build tools (Latest)
  - Windows 11 SDK (10.0.22621.0)
- rustup-msvc
  - Via [scoop](https://scoop.sh/): `scoop install rustup-msvc`
  - Via [rustup-init](https://www.rust-lang.org/tools/install?platform_override=win)

After setting up all the requirements:

- Add MSVC tools to Path:

  ```ps1
  [Environment]::SetEnvironmentVariable("PATH", $env:PATH + ";C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\MSVC\14.50.35717\bin\Hostx64\x64", [EnvironmentVariableTarget]::User)
  ```

- Install [Rust toolchain](https://rust-lang.github.io/rustup/concepts/toolchains.html):

  <!-- markdownlint-disable-next-line MD040 -->
  ```
  rustup install nightly-2023-08-18 --no-self-update
  rustup default nightly-2023-08-18
  ```

### Linux / WSL

> [!NOTE]
> This was tested on Debian so the packages and instructions may differ.

- Build tools:

  ```sh
  $ sudo apt install build-essential pkg-config
  # Cross compiler for x86_64-pc-windows-gnu
  $ sudo apt install gcc-mingw-w64
  # gtk3
  $ sudo apt install libgtk-3-dev
  ```

- [rustup](https://rust-lang.org/tools/install/?platform_override=linux):

  ```sh
  # Remove previous Rust versions
  $ sudo apt remove --purge rustc cargo
  $ sudo apt autoremove --purge
  # Install rustup
  $ curl https://sh.rustup.rs -sSf | sh
  ```

After setting up all the requirements:

- Add Rust to Path:

  ```sh
  source "$HOME/.cargo/env"
  ```

- Install [Rust toolchain](https://rust-lang.github.io/rustup/concepts/toolchains.html):

  <!-- markdownlint-disable-next-line MD040 -->
  ```
  rustup install nightly-2023-08-18 --no-self-update
  rustup default nightly-2023-08-18
  ```

## Compiling

- Run `cargo build` to compile a development build
  - The output will be inside `target/debug`
- Run `cargo build --release` to compile a production build
  - The output will be inside `target/release`
