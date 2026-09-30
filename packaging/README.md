# Packaging Kelp

Kelp ships as a Homebrew **cask**, because it is a GUI app: the cask puts `Kelp.app` in
`/Applications` (Spotlight and Launchpad find it) and the `binary` stanza links the `kelp` CLI
onto your PATH. `kelp.rb` in this folder is a reference copy; the release workflow generates the
real one with the right version and sha256.

## Release flow (automated)

1. Bump `version` in the workspace `Cargo.toml`.
2. Update `site/index.html` (version pill on every page, anything new; `scripts/check-site.py` flags stale pills) and `LEDGER.md`.
3. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
4. `.github/workflows/release.yml` checks the tag matches `Cargo.toml`, runs the tests, builds a
   **universal** binary (arm64 + x86_64), wraps it in `Kelp.app`, zips it with `ditto`, cuts the
   GitHub Release, generates the cask, and commits it to `Hobo-Ware/homebrew-tap` as
   `Casks/kelp.rb`. A manual re-run for an older tag never rolls the tap back.
5. Installed copies pick it up on their own: Kelp checks the latest release every 6 hours and,
   when it was installed with Homebrew, runs `brew upgrade --cask hobo-ware/tap/kelp` in the
   background and offers "Restart to update".

## One-time setup

Tap deploy key (`github.token` cannot push to another repo):

```sh
ssh-keygen -t ed25519 -N "" -C "kelp release -> homebrew-tap" -f tap_key
gh repo deploy-key add tap_key.pub -R Hobo-Ware/homebrew-tap -t "kelp release" -w
gh secret set HOMEBREW_TAP_DEPLOY_KEY -R Hobo-Ware/kelp < tap_key
rm tap_key tap_key.pub
```

Then:

```sh
brew install hobo-ware/tap/kelp   # installs to /Applications and links the kelp CLI
```

## Signing and notarization (optional)

Same setup as stdusk (see `Hobo-Ware/stdusk/packaging/README.md`): when the five secrets
`MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`, `NOTARY_KEY_ID`, `NOTARY_ISSUER` and `NOTARY_KEY` are
set on this repo, releases are Developer ID signed, notarized and stapled, and the cask drops its
quarantine-strip postflight. Without them, builds ship ad-hoc signed and the cask strips
`com.apple.quarantine` after install so Gatekeeper does not block the launch.

## Icon

`crates/kelp/assets/Kelp.icns` is generated from `crates/kelp/assets/icon.svg` (which is built
from `mascot.svg`) by `scripts/make-icon.sh`, with every size compressed through pngquant and
oxipng. Rerun it after changing the SVG; CI uses the committed `.icns`.
