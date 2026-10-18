# Package managers

Every release also produces files for Homebrew and winget. `scripts/render-packages.sh` fills the
templates in `packaging/` with the version and the checksums of the release; the `packages` job of
`.github/workflows/release.yml` runs it after the release is published.

| Where | What | Needs |
|---|---|---|
| Homebrew, macOS | cask `hydramerge/tap/hydra-merge` | the secret `TAP_TOKEN` |
| Homebrew, Linux | formula `hydramerge/tap/hydra-merge` | the secret `TAP_TOKEN` |
| winget, Windows | package `HydraMerge.HydraMerge` | a pull request to `microsoft/winget-pkgs` |

## Homebrew

The tap is [HydraMerge/homebrew-tap](https://github.com/HydraMerge/homebrew-tap). Its `Casks/` and
`Formula/` are written by the release workflow; change the templates in `packaging/homebrew/` here,
not the files there.

The workflow cannot push to another repository with its own token, so it needs a secret:

1. GitHub → Settings → Developer settings → Fine-grained personal access tokens → Generate new token.
2. Resource owner `HydraMerge`, repository access *Only select repositories* → `homebrew-tap`.
3. Repository permissions: **Contents: Read and write**. Nothing else.
4. Save the token as the secret: `gh secret set TAP_TOKEN --repo HydraMerge/hydra-merge`.

Without the secret the job still renders the files and attaches them to the run as the artifact
`package-manifests`; the tap is left as it is.

The cask clears the quarantine flag of the not notarized app in `/Applications`. Homebrew 7 prefers
`postflight_steps`, which cannot name the app directory, so the step uses the literal default path.
A custom `--appdir` keeps the flag; the cask's caveat says how to clear it.

Check a change to a template before releasing (a temporary tap, nothing is installed):

```sh
sh scripts/render-packages.sh 0.1.18 SHA256SUMS out        # SHA256SUMS of that release
brew tap-new --no-git local/check
cp out/homebrew/Casks/*.rb "$(brew --repository local/check)/Casks/"
brew audit --cask --strict --online local/check/hydra-merge
brew untap local/check
```

## winget

winget installs a portable app from a `.zip`, so the release carries `hydra-merge-<tag>-<arch>-pc-windows-msvc.zip`
next to the `.tar.gz` the app updates itself from. The rendered manifests are in the artifact
`package-manifests`, under `winget/manifests/h/HydraMerge/HydraMerge/<version>/`.

First submission (once, by hand):

1. Download the artifact `package-manifests` of the release run.
2. On Windows, check it: `winget validate --manifest <version folder>` and
   `winget install --manifest <version folder>` (enable local manifests with
   `winget settings --enable LocalManifestFiles`).
3. Fork [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs), copy
   `manifests/h/HydraMerge/HydraMerge/<version>/` into the fork, and open a pull request titled
   `New package: HydraMerge.HydraMerge version <version>`. Its checks run the installer in a sandbox.
4. When it is merged, `winget install HydraMerge.HydraMerge` works and the README can say so.

Later versions: `wingetcreate update HydraMerge.HydraMerge --version <version> --urls <x64 zip>|x64 <arm64 zip>|arm64 --submit`
(or the same copy and pull request as above). A job that does this on every release needs a token
with the `public_repo` scope; add it when the first package is accepted.
