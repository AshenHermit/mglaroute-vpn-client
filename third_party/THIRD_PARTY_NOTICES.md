# Third-party notices

This build bundles the following third-party components. Their licenses and
notices are preserved in `third_party/licenses/` and copied into the bundled
anti-DPI resource directory by `scripts/stage-antiddpi.mjs`.

## Xray-core

- Upstream: https://github.com/XTLS/Xray-core
- License: MPL-2.0 (SPDX `MPL-2.0`; full text in `licenses/Xray-core-MPL-2.0.txt`)
- Binary is downloaded with sha256 (`.dgst`) verification and bundled as a
  Tauri `externalBin`. Not modified.

## zapret-win-bundle / bol-van zapret2

- Upstream: https://github.com/bol-van/zapret-win-bundle
- Pinned commit: `6eb463a6758fb48cd101bc55dfd057e6e9d98af1`
- Pinned archive sha256: `5f828239310fed0da587bc09eaa22742615b2a85cf41f55ff28128e22231c6de`
- zapret2 engine license: MIT (SPDX `MIT`)
- Wrapper/catalogue (Igareck) components: GPL-3.0-or-later.

## WinDivert

- Upstream: https://reqrypt.org/windivert.html
- License: LGPL-3.0-or-later OR GPL-2.0-or-later
  (SPDX `LGPL-3.0-or-later OR GPL-2.0-or-later`).

## Cygwin runtime

- Upstream: https://cygwin.com/
- License: LGPL-3.0-or-later with linking exception
  (SPDX `LGPL-3.0-or-later-with-exception`).

## LuaJIT

- Upstream: https://luajit.org/
- License: MIT (SPDX `MIT`).

## Licensing note

No third-party notice was removed or rewritten. `scripts/stage-antiddpi.mjs`
copies every `LICENSE`, `LICENCE`, and `COPYING` file found in the pinned
upstream archive into the bundled resource tree alongside this manifest.
Anti-DPI is bundled but not invoked in the MVP.
