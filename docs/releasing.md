---
title: Releasing
nav_order: 14
---

# Releasing Kea

Ordinary changes land through pull requests to **`develop`**. Pushes to `develop`
and `main`, and PRs targeting either branch, run CI and build documentation.
Maintainers promote tested changes to **`main`** for release. Only matching version
tags whose commits are reachable from `main` can publish a release.

## First public publication

Preparation is separate from publication. Before making the repository public:

1. Review the working tree, tracked files and existing Git history/branches for
   material that must not be public. Ignoring a file does not remove old commits.
2. Finish the [compatibility matrix](terminal-compatibility-alpha.md) and record
   actual platform results. Automated checks do not replace real-desktop acceptance.
3. Commit the prepared `0.0.1-alpha.1` version and lockfile on `main`, create
   `develop` at the same commit, then push both when ready:

   ```sh
   git push origin main develop
   ```

4. Make the repository public in GitHub Settings → General → Change visibility.
   This exposes the existing history and remote branches too.
5. Set the default branch to `develop` so ordinary PRs target it. Add branch
   protection/rulesets for `develop` and `main`: require PRs, disallow force pushes
   and deletion, and select CI checks after their first successful runs.
   Expected checks are `engine (ubuntu-latest)`, `engine (macos-latest)`,
   `engine (windows-latest)`, `desktop-linux`, `desktop-windows`, and Documentation's
   `build`. Keep release promotion to `main` possible for maintainers.
6. Enable Actions, GitHub Pages (source **GitHub Actions**) and private vulnerability
   reporting. Set the repository description and website to
   `https://manuelzierl.github.io/kea/`.
7. Run CI and Documentation on `main` and CI on `develop` from the Actions tab.
   Wait for successful runs, inspect the deployed site, and verify CI with a real
   PR targeting `develop` before pushing the release tag.

## Local validation

Use the checks in the
[contributor guide](https://github.com/ManuelZierl/kea/blob/main/CONTRIBUTING.md).
On Ubuntu, install the source-build dependencies plus:

```sh
sudo apt-get install -y xvfb xdotool xclip imagemagick mesa-vulkan-drivers dbus-x11 fonts-ubuntu
```

After `cargo build --locked -p kea-app`, run the same isolated graphical test as CI:

```bash
lvp_icds=(/usr/share/vulkan/icd.d/lvp_icd*.json)
test -e "${lvp_icds[0]}"
env -u WAYLAND_DISPLAY -u DISPLAY \
  MESA_VK_WSI_FORCE_SW=1 VK_ICD_FILENAMES="${lvp_icds[0]}" \
  LIBGL_ALWAYS_SOFTWARE=1 \
  timeout 180s xvfb-run -a -s '-screen 0 1280x900x24 +extension DRI3' \
  dbus-run-session -- bash scripts/smoke-linux.sh
```

The coordinate-based fixture requires Ubuntu font metrics; the script checks
that prerequisite instead of clicking fields laid out with a different fallback
font. It writes diagnostic output to the ignored `smoke-artifacts/` directory.
Use the [manual checklist](manual-testing.md) for real keyboard, clipboard, IME,
accessibility and shell/TUI acceptance on release platforms.

## Tag and publish

Update `[workspace.package].version` in `Cargo.toml` and refresh `Cargo.lock` with
`cargo check --offline -p kea-core`. Review the lockfile diff;
only intended version/dependency changes should be present. Update release notes
in `CHANGELOG.md` (replace the prepared heading with the release date) and remove
the README's planned-release wording, commit, validate, and promote to `main`
before tagging.

For the first alpha, after hosted CI and acceptance pass:

```sh
git switch main
git pull --ff-only origin main
git tag -a v0.0.1-alpha.1 -m "Kea v0.0.1-alpha.1"
git push origin v0.0.1-alpha.1
```

The tag push runs the complete CI matrix, builds unsigned archives for Linux
x86_64, macOS (the hosted runner's architecture, included in the filename) and
Windows x86_64, and publishes a GitHub release only if all
required jobs pass. Version and `main` ancestry are checked before publication.
Versions containing a hyphen are marked **prerelease** and do not become Latest.
Archives include the executable, README, license and release notes. `SHA256SUMS`
uses archive basenames so it can be verified from the download directory:

```sh
sha256sum -c SHA256SUMS
```

All archives must be downloaded for that command to check the complete manifest.
Inspect the release assets and test extracted binaries on their target platforms.
These are unsigned binaries, not native installers or a cross-platform certification.
Never move a published release tag; fix problems in a new version. Re-running a
failed publication can replace assets for the same tag, so avoid rerunning a
successful published release without a specific reason.

## Documentation site

The site uses Just the Docs, pinned in `docs/_config.yml`, and GitHub's
`actions/jekyll-build-pages` build environment. Markdown links are converted by
`jekyll-relative-links`. Every page supplies its navigation title/order; the
landing page is `docs/index.md`. No custom JavaScript or stylesheet is required.

The Documentation workflow builds both branches and their PRs. Only `main` deploys
to Pages. Build jobs do not need Pages to be enabled; deployment does. After enabling
Pages, rerun Documentation on `main` if its initial deployment failed.

To preview with Docker from the repository root using the same builder as CI:

```sh
docker run --rm --entrypoint /bin/bash \
  -v "$PWD:/github/workspace" -w /github/workspace \
  ghcr.io/actions/jekyll-build-pages:latest \
  -c 'jekyll build --source docs --destination docs/_site --trace'
python3 -m http.server 4000 --directory docs/_site
```

The generated site uses the `/kea` project prefix. For a root-path local preview,
append `--baseurl ""` to the Jekyll command. Generated output stays ignored.
