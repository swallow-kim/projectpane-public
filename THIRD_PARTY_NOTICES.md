# Third-party notices

ProjectPane-owned code and documentation are licensed under [MIT](LICENSE). This does not change the licenses of dependencies, copied upstream material, system components, or external applications.

## Current source inventory (1.2.1)

The [locked dependency inventory](docs/dependency-licenses-1.2.1.md) records 88 npm dependencies and 250 Rust packages from the Windows desktop dependency graph, including build/test tools. It is not a claim that every listed package is linked into the executable. The [collected license texts](docs/third-party-license-texts.md) retain upstream notices available for the conservatively classified runtime graph. Optional npm packages for other platforms are listed even when not installed locally.

- React, React DOM, scheduler, Zod and the Tauri API retain their upstream licenses and copyright notices.
- Lucide uses ISC and includes Feather-derived icons under MIT. [The complete upstream notice](docs/lucide-license.txt) is preserved, including the Feather attribution. `public/lucide-license.txt` is the same text.
- The modified NSIS template is derived from Tauri CLI 2.12.1. Its source version and original hash are recorded in the template header; [Tauri's original MIT notice](src-tauri/installer/LICENSE-MIT) remains unchanged.
- The simple four-pane application artwork originates in this repository's initial implementation. There is no bundled font file; the UI uses system fonts. Repository provenance is not independent proof of ownership or a trademark clearance.

## MPL and other licenses

Some Rust dependencies use MPL-2.0, including `cssparser`, `dtoa-short`, `option-ext`, and `selectors`; `cssparser-macros` is a build dependency. Their source remains under MPL, not ProjectPane's MIT. Exact version source-download links are in the inventory. No local patch/vendor override of these dependencies was found in the audited manifests. Recipients of a binary containing these components must be told where to obtain the corresponding MPL source, including any future modifications. See [MPL section 3](https://www.mozilla.org/en-US/MPL/2.0/#responsibilities) and [Mozilla's distribution FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/).

The graph also contains Apache-2.0, BSD, ISC, Unicode-3.0, Zlib and dual-license expressions. An `AND` expression requires both sets of terms; an `OR` expression permits a license choice. Preserve applicable attribution/NOTICE text rather than relabeling all dependencies MIT. No GPL/AGPL dependency was found in these two current lockfile inventories; this is not a determination about every historical installer component.

## Windows packaging boundary

NSIS, its compression modules and installer plug-ins have their own terms. The upstream [NSIS license page](https://nsis.sourceforge.io/License) distinguishes zlib/libpng, bzip2 and CPL components. Audit the actual pinned toolchain and embedded plug-ins for each distributed installer; the template's MIT license alone is insufficient evidence for all of them.

WebView2 Runtime is not bundled in the current EXE/ZIP. It is a separately installed Microsoft component; the installer may download Microsoft's bootstrapper. The Rust bindings' MIT license does not relicense Microsoft's SDK loader libraries or runtime. Their actual redistribution terms and notices must be checked with the selected SDK/toolchain before public binary distribution. See [Microsoft's deployment guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution).

Installed archive applications are launched externally and are not bundled or relicensed by ProjectPane.

## Before publishing binaries

The existing v0.1.3–v1.1.0 assets were not rebuilt or changed by this license commit. In particular, their ZIP files do not include a complete dependency-notice/source-availability bundle, and their old documentation predates this MIT grant. Adding files to the source repository does not update those downloads.

- Generate notices from the exact lockfiles and packaging inputs of the release, retaining per-component copyright and any required NOTICE text.
- Include ProjectPane LICENSE, third-party notices, applicable license texts, and exact MPL source locations in both installer and ZIP distribution.
- Verify NSIS plug-in/compression and WebView2 SDK loader requirements; do not infer their terms from the application manifest.
- Check that all licensed source locations remain available and that any modified covered source is supplied.
- Inspect packaged files and the About/help access path; source-only notices are not proof of binary compliance.

See [distribution requirements](docs/distribution.md) before shipping binaries. Source notices do not certify installer or SDK redistribution compliance.
