# License texts

License inventory for bundled third-party components:

| Component | License | SPDX |
| --- | --- | --- |
| Xray-core | MPL-2.0 | `MPL-2.0` |
| zapret2 engine | MIT | `MIT` |
| zapret-win-bundle wrapper / Igareck catalogues | GPL-3.0-or-later | `GPL-3.0-or-later` |
| WinDivert | LGPL-3.0-or-later OR GPL-2.0-or-later | `LGPL-3.0-or-later OR GPL-2.0-or-later` |
| Cygwin runtime | LGPL-3.0-or-later with linking exception | `LGPL-3.0-or-later-with-exception` |
| LuaJIT | MIT | `MIT` |

Full license texts are vendored here: `GPL-2.0.txt`, `GPL-3.0.txt`,
`LGPL-3.0.txt`, `MIT.txt`, `Xray-core-MPL-2.0.txt`, `WinDivert-LICENSE`,
`LuaJIT-COPYRIGHT`, `cygwin-licensing.html`, `zapret-LICENSE.txt`, and
`zapret2-LICENSE.txt`. `scripts/stage-antiddpi.mjs` copies these (and any
additional `LICENSE`/`LICENCE`/`COPYING` files found in the pinned upstream
archive) into the bundled resource tree during `pnpm binaries:fetch`. See
`../THIRD_PARTY_NOTICES.md` for upstream URLs and the pinned commit.
