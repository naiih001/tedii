# Release Engineering Plan for `tedii`

## Summary
Add a basic release pipeline that keeps versioned releases, changelog notes, and platform binaries in sync. The workflow is centered on a dedicated `release` branch, with `release-plz` managing release PRs, version bumps, changelog generation, and GitHub release publishing.

## Key Changes
- Add release automation under `.github/`:
  - `release-plz` config to define release branch behavior and changelog generation
  - a release workflow that cuts tagged releases from the `release` branch
  - a build workflow that packages binaries for `linux` and `macOS`
- Keep release notes generated from merged commits, not maintained manually.
- Use semantic version tags like `v0.1.0`.
- Publish release artifacts for:
  - Linux
  - macOS
- Keep the package name and artifact names aligned with the crate name `tedii`.
- Add checksums to release assets so downloads can be verified.

## Test Plan
- Verify the release config is valid and points at the `release` branch.
- Inspect the release workflow behavior for version/tag generation.
- Confirm the build workflow produces platform binaries for Linux and macOS.
- Confirm the release notes/changelog are generated from the merged history and included in the GitHub Release payload.

## Assumptions
- Release automation should be merge-driven through a dedicated `release` branch named `release`.
- `release-plz` is the release bot of record.
- Changelog content is generated automatically from commits.
- The first release flow should stay basic: no code signing, no package-manager publishing, and no Windows artifact yet unless requested later.
