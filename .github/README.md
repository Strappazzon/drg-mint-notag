<!-- markdownlint-disable MD033 MD041 -->

<div align="center">
  <img width="96" src="../assets/header-logo.png" alt="Logo">
</div>

<div align="center">
  <strong>mint-notag</strong>
</div>

<p align="center">
  <em>Third-party mod integration tool for Deep Rock Galactic.</em>
</p>

## About

This is my fork of mint with the following changes from [upstream](https://github.com/trumank/mint/tree/832f7db):

- Omit `[MODDED]` prefix from public lobby name
- Renamed program to `mint-notag` from `DRG Mod Integration`
  - Logs and config paths will still use the old name
- Added an [icon](../assets/icon.ico) to title bar, taskbar and executable
- Changed main window size and position
- Improved messages and buttons description
- Improved update modal
  - Added Download button that will open the release page
  - Close button can be closed immediately
  - Fetch remote images in the release notes
- Shortened approval labels
- Added <kbd>CTRL</kbd> + <kbd>Q</kbd> keyboard shortcut to quit the program

For more information on mint usage see the [original README](https://github.com/trumank/mint/blob/-/README.md).

## Preview

![mint-notag GUI](../assets/screenshot.png)

## Download

The latest release is available on the [Releases page](https://github.com/Strappazzon/drg-mint-notag/releases/latest).

## Contributing

If you are interested in contributing directly to this repository, please see:

- [Contribution Guidelines](./CONTRIBUTING.md)
- [Code of Conduct](https://github.com/Strappazzon/.github/blob/-/CODE_OF_CONDUCT.md)

## Licensing

The code is open source under the terms of the [MIT License](../LICENSE.txt).

App icon from [Icons8](https://icons8.com/icon/6zZTdWRZoWil/rock).

By contributing to this repository, you agree that the content you contribute may be provided under the terms of the [MIT License](../LICENSE.txt).
