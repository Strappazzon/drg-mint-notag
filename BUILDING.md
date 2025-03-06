# Building mint-notag

This guide should assist you in compiling and running mint-notag.

> [!NOTE]
> Instructions are for Windows as I don't have access to Linux.

## Requirements

- Visual Studio with the following individual components:
  - MSVC v143 - VS 2022 C++ x64/x86 build tools (Latest)
  - Windows 11 SDK (10.0.22621.0)
- rustup-msvc
  - Via [scoop](https://scoop.sh/): `scoop install rustup-msvc`
  - Via [rustup-init](https://www.rust-lang.org/tools/install?platform_override=win)

After setting up all the requirements:

- Add MSVC tools to Path:

  ```ps1
  [Environment]::SetEnvironmentVariable("PATH", $env:PATH + ";C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.43.34808\bin\Hostx64\x64", [EnvironmentVariableTarget]::User)
  ```

- Install [Rust toolchain](https://rust-lang.github.io/rustup/concepts/toolchains.html):

  <!-- markdownlint-disable-next-line MD040 -->
  ```
  rustup install nightly-2023-08-18 --no-self-update
  rustup default nightly-2023-08-18
  ```

## Building with cargo

- Run `cargo build` to compile a development build
  - The output will be inside `target/debug`
- Run `cargo build --release` to compile a production build
  - The output will be inside `target/dist`
