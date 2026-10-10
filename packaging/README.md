# Packaging

- `windows/installer.iss`: Inno Setup 6 installer (per-user, unsigned), built by `.github/workflows/desktop-builds.yml`.
- `macos/Info.plist`: app bundle metadata. The workflow assembles `Hook Echo.app`, signs it ad hoc (Apple Silicon refuses to run unsigned code) and wraps it in a `.dmg`.

Both builds are unsigned. The call-for-testing issue explains how to open them the first time.
