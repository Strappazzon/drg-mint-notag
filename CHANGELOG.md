# Change Log

<!-- next-header -->

## [Unreleased] - ReleaseDate

## [0.2.12] - 2025-03-05

- Improve labels and messages
- Improve delete button
- Remove "Optional" label
- Add icon to taskbar and title bar
- Center main window on screen
- Set min window size
  - 630 x 175
- Improve updater
- Add about window
- Change description of mod toggle switch
- Reduce release size by removing debug symbols
- Indicate which mod causes integration error when possible (merge from upstream)
- Detour DoesSaveGameExist (merge from upstream)

## [0.2.11] - 2025-03-04

- Remove `[MODDED]` prefix from public lobby name
- Rename program to `mint-notag` from `DRG Mod Integration`
  - Logs and config paths will still use the old name
- Add CTRL + Q keyboard shortcut to quit the program
- Better buttons description
- Add program icon
- Update modal will show without delays
- Increase initial window size
- Shorten approval labels

## [0.2.10] - 2023-08-18

- Many small improvements to the GUI
- Add simple in game UI to show local and remote integration version and active mods
- Add experimental mod linting to assist with common mod issues such as conflicts ([#55](https://github.com/trumank/drg-mod-integration/pull/55))
- Microsoft Store: Fix mods being unable to write custom save files ([#58](https://github.com/trumank/drg-mod-integration/issues/58))
- Fix `profile` CLI command not respecting mod's `enable` flag

## [0.2.9] - 2023-08-11

- Update `egui_dnd` which makes dragging and re-ordering mods significantly smoother
- Restore modding subsystem config upon uninstalling to prevent all mods getting enabled and kicking the user to sandbox
- Fix regression introduced by case sensitive path fix ([#36](https://github.com/trumank/drg-mod-integration/issues/36))

## [0.2.8] - 2023-08-05

- Fix `*.ushaderbytecode` files not being filtered out and causing crash on load
- Fix including same asset paths with different casings causing Unreal Engine to load neither ([#29](https://github.com/trumank/drg-mod-integration/issues/29))

<!-- next-url -->
[Unreleased]: https://github.com/Strappazzon/drg-mint-notag/compare/v0.2.12...HEAD
[0.2.12]: https://github.com/Strappazzon/drg-mint-notag/compare/v0.2.11...v0.2.12
[0.2.11]: https://github.com/Strappazzon/drg-mint-notag/compare/832f7db...v0.2.11
[0.2.10]: https://github.com/trumank/drg-mod-integration/compare/v0.2.9...v0.2.10
[0.2.9]: https://github.com/trumank/drg-mod-integration/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/trumank/drg-mod-integration/compare/v0.2.7...v0.2.8
