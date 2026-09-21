# Hoenn Sessions Windows installer

This is the private-pilot, per-user MSI. It installs only the stable MSI-owned
bootstrapper, the onboarding fallback, nonsecret pilot configuration, notices,
and a Start Menu shortcut below `%LOCALAPPDATA%\Programs\Hoenn Sessions`.

Runtime generations, saves, recovery evidence, mutable state, and Windows
Credential Manager entries live below `%LOCALAPPDATA%\Hoenn Sessions` and are
never included in the MSI. Uninstall therefore leaves player data intact; an
explicit in-app operation is required to delete it.

The bootstrap has no network or authentication responsibility. It opens only a
cryptographically verified accepted generation selected by the updater, or it
starts the MSI-owned onboarding executable. The trust root is compiled into
the executable from build inputs and is not read from this file.

## Development build

Pass `-UnsignedDevelopment` to `build-installer.ps1` for a local unsigned MSI.
Unsigned output is intentionally marked as development-only and must not be
distributed.

The temporary private pilot may pass `-UnsignedPrivatePilot`. That output is
explicitly labelled `unsigned-private-pilot` in provenance and must remain a
private, invite-only artifact. Windows will identify it as an unknown publisher
and may require testers to choose **Run anyway**. This exception does not disable
Ed25519 verification of downloaded runtime generations.

Distribute `HoennSessions-UNSIGNED-PRIVATE-PILOT.msi`, `hashes.json`, and
`provenance.json` together from the same authenticated GitHub Actions run.
Before choosing **Run anyway**, verify that provenance names the expected
repository, commit, run id, and attempt, then verify the MSI SHA-256 against
`hashes.json`. Do not rename or forward the unsigned MSI without both metadata
files; its conspicuous filename is part of the pilot safety boundary.

Normal release builds require an Authenticode certificate and RFC 3161
timestamp URL; the script signs both EXEs before MSI packaging, then signs the
MSI and emits SHA-256 hashes and provenance beside the package.
