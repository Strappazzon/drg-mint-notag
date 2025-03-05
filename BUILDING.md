<!-- markdownlint-disable MD040 -->

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
- (Optional) cargo-dist
  - Via [scoop](https://scoop.sh/):

    ```
    scoop bucket add strappazzon https://github.com/Strappazzon/scoop
    scoop install strappazzon/cargo-dist-007
    ```

After setting up all the requirements:

- Add MSVC tools to Path:

  ```ps1
  [Environment]::SetEnvironmentVariable("PATH", $env:PATH + ";C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.43.34808\bin\Hostx64\x64", [EnvironmentVariableTarget]::User)
  ```

- Install [Rust toolchain](https://rust-lang.github.io/rustup/concepts/toolchains.html):

  ```
  rustup install nightly-2023-08-18 --no-self-update
  rustup default nightly-2023-08-18
  ```

## Getting the source code

Clone the repository via Git:

```cmd
git clone https://github.com/Strappazzon/drg-mint-notag.git drg-mint-notag
cd drg-mint-notag
```

Alternatively, you can clone the repository via any Git client, or download a zip archive of the repository [from here](https://github.com/Strappazzon/drg-mint-notag/archive/master.zip).

## Building with cargo

Run `cargo build` to compile a development build.

The output will be inside `target/debug`.

## Building with cargo-dist

To create a production build with cargo-dist, run `cargo dist build --artifacts=local --target=x86_64-pc-windows-msvc`.

The output will be inside `target/distrib/<bin>-<target triple>/`.
