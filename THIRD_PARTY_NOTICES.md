# Third-party notices

ProjectPane-owned code and documentation are licensed under [MIT](LICENSE). This does not change the licenses of dependencies, copied upstream material, system components, or external applications.

## Current source inventory (1.5.1)

The [locked dependency inventory](docs/dependency-licenses-1.5.1.md) records 377 npm/Rust packages from the Windows desktop dependency graph, including build/test tools. It is not a claim that every listed package is linked into the executable. The [collected license texts](docs/third-party-license-texts.md) retain upstream notices for the conservatively classified runtime graph. Optional npm packages for other platforms are listed even when not installed locally. The updater is Tauri updater 2.14.1 (Apache-2.0 OR MIT); it does not relicense the application or the TLS/signature libraries.

- React, React DOM, scheduler, Zod and the Tauri API retain their upstream licenses and copyright notices.
- Lucide uses ISC and includes Feather-derived icons under MIT. [The complete upstream notice](docs/lucide-license.txt) is preserved, including the Feather attribution. `public/lucide-license.txt` is the same text.
- The modified NSIS template is derived from Tauri CLI 2.12.1. Its source version and original hash are recorded in the template header; [Tauri's original MIT notice](src-tauri/installer/LICENSE-MIT) remains unchanged.
- The simple four-pane application artwork originates in this repository's initial implementation. There is no bundled font file; the UI uses system fonts. Repository provenance is not independent proof of ownership or a trademark clearance.

## MPL and other licenses

Some Rust dependencies use MPL-2.0, including `cssparser`, `dtoa-short`, `option-ext`, and `selectors`; `cssparser-macros` is a build dependency. Their source remains under MPL, not ProjectPane's MIT. Exact version source-download links are in the inventory. No local patch/vendor override of these dependencies was found in the audited manifests. Recipients of a binary containing these components must be told where to obtain the corresponding MPL source, including any future modifications. See [MPL section 3](https://www.mozilla.org/en-US/MPL/2.0/#responsibilities) and [Mozilla's distribution FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/).

The graph also contains Apache-2.0, BSD, ISC, Unicode-3.0, Zlib and dual-license expressions. An `AND` expression requires both sets of terms; an `OR` expression permits a license choice. Preserve applicable attribution/NOTICE text rather than relabeling all dependencies MIT. No GPL/AGPL dependency was found in these two current lockfile inventories; this is not a determination about every historical installer component.

## Windows packaging boundary

NSIS 3.11 and its System, nsDialogs, StartMenu and NSISdl plug-ins retain the [NSIS COPYING](docs/licenses/NSIS-COPYING.txt) and [NSISdl notice](docs/licenses/NSISdl-License.txt). These distinguish zlib/libpng, bzip2 and CPL compression components. The corresponding unmodified NSIS source is [NSIS 3.11 source](https://sourceforge.net/projects/nsis/files/NSIS%203/3.11/nsis-3.11-src.tar.bz2/download). The additional nsis_tauri_utils 0.5.3 plug-in matches the official release SHA1 75197FEE3C6A814FE035788D1C34EAD39349B860; its [MIT notice](docs/licenses/nsis-tauri-utils-MIT.txt) is preserved. ProjectPane modifies the installer template and safety hooks; those changes are in this repository.

WebView2 Runtime is not bundled. It is a separately installed Microsoft component; initial installation may download Microsoft's bootstrapper. The static x64 loader in webview2-com-sys 0.39.1 matches Microsoft.Web.WebView2 SDK 1.0.3800.47 byte-for-byte (SHA256 89C6B872783B8F6C3CEDBFF618ADB42082D455C615453AE10CFC753F1E8F25D8). Its [Microsoft license](docs/licenses/Microsoft-WebView2-SDK-LICENSE.txt) and [NOTICE](docs/licenses/Microsoft-WebView2-SDK-NOTICE.txt) are included; the Rust binding MIT does not relicense them. See [Microsoft's deployment guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution).

Installed archive applications are launched externally and are not bundled or relicensed by ProjectPane.

## Binary distribution checks

The 1.5.1 installer configuration includes the current LICENSE, this notice, dependency inventory, collected license texts and packaging-component notices under the installed `licenses` directory. Inclusion is verified against each final package; adding source notices alone does not change previously distributed binaries.

- Generate notices from the exact lockfiles and packaging inputs of the release, retaining per-component copyright and any required NOTICE text.
- Include ProjectPane LICENSE, third-party notices, applicable license texts, and exact MPL source locations in both installer and ZIP distribution.
- Verify NSIS plug-in/compression and WebView2 SDK loader requirements; do not infer their terms from the application manifest.
- Check that all licensed source locations remain available and that any modified covered source is supplied.
- Inspect packaged files and the About/help access path; source-only notices are not proof of binary compliance.

See [distribution requirements](https://github.com/swallow-kim/projectpane-public/blob/main/docs/distribution.md) before shipping binaries. Source notices do not certify installer or SDK redistribution compliance.
