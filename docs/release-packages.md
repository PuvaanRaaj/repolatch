# Release packages

Every successful push or merge to **main** creates a unique GitHub prerelease. The release tag uses **main-RUN-SHA**, where RUN is the release-workflow run number and SHA is the first seven characters of the commit.

Continuous-build packages are unsigned and are not notarized. They are intended for testing until a signed release process is added.

| Package | Description |
| --- | --- |
| **repolatch-cli-linux-x86_64.tar.gz** | RepoLatch CLI for 64-bit Linux. |
| **repolatch-cli-macos-arm64.tar.gz** | RepoLatch CLI for Apple silicon Macs. |
| **repolatch-cli-windows-x86_64.zip** | RepoLatch CLI for 64-bit Windows. |
| **repolatch-desktop-linux-x86_64.AppImage** | Portable Linux desktop application. |
| **repolatch-desktop-linux-x86_64.deb** | Desktop package for Debian and Ubuntu systems. |
| **repolatch-desktop-linux-x86_64.rpm** | Desktop package for RPM-based Linux systems. |
| **repolatch-desktop-macos-arm64.dmg** | macOS disk image for Apple silicon Macs. |
| **repolatch-desktop-windows-x86_64.msi** | Windows MSI installer. |
| **repolatch-desktop-windows-x86_64-setup.exe** | Windows NSIS installer. |
| **SHA256SUMS.txt** | SHA-256 checksums for all packages in the release. |

The workflow publishes a release only after every expected package is present and non-empty. Rerunning a completed workflow updates the release for that commit instead of creating another tag.
