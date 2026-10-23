# Licensing

yAIst is licensed under **MIT OR Apache-2.0**. This page is for packagers and
legal reviewers (Cavil, the openSUSE packaging skill): what is in the source,
what ends up in the binaries, and what is only downloaded at runtime.

## Where the facts live

| File | What it says |
|---|---|
| `LICENSE-MIT`, `LICENSE-APACHE` | The project licence, at the repository root |
| `LICENSES/` | Every licence text used anywhere in the tree (REUSE layout) |
| `REUSE.toml` | Per-file copyright and SPDX identifier; `reuse lint` checks it |
| `NOTICE` | Attribution that redistributors must keep |
| `Cargo.toml` (workspace) | `license = "MIT OR Apache-2.0"`, inherited by every crate |
| `deny.toml` | Allowed crate licences; `cargo deny check licenses` enforces it |
| `scripts/license-report.sh` | Linked crates per binary and the suggested `License:` tag |

## The source tree

| Path | Licence | Notes |
|---|---|---|
| everything not listed below | MIT OR Apache-2.0 | yAIst's own work |
| `apps/yaist-gui/src/theme.rs`, `widgets.rs` | MIT OR Apache-2.0 | adapted from the ArtCraft launcher; both copyright holders in `REUSE.toml` |
| `assets/fonts/*.ttf` | OFL-1.1 | Inter, Archivo, Geist Mono; texts in `assets/fonts/OFL-*.txt` |

No ArtCraft trademarks (name, logo, icons) are used. The look borrows only the
layout and widget style, in the openSUSE colour palette.

## The binaries

Rust links its dependencies statically, so each binary's licence is yAIst's
plus whatever its crates add. Output of `scripts/license-report.sh` at the
time of writing (x86_64 Linux target, normal dependencies only):

| Binary | Crates linked | Suggested `License:` |
|---|---|---|
| `yaist` (TUI and CLI) | 152 | `(MIT OR Apache-2.0) AND Unicode-3.0 AND Zlib` |
| `yaist-gui` | 251 | `(MIT OR Apache-2.0) AND ISC AND OFL-1.1 AND Unicode-3.0 AND Zlib` |

- **No copyleft is forced.** Two crates offer a copyleft licence as one `OR`
  choice: `r-efi` (`MIT OR Apache-2.0 OR LGPL-2.1-or-later`) and `self_cell`
  (`Apache-2.0 OR GPL-2.0-only`). The permissive alternative applies.
- **OFL-1.1** in `yaist-gui` comes from the bundled fonts. Building with
  `--no-default-features` (no `bundled-fonts`) drops the embedded fonts; the
  binary then reads them from `/usr/share/yaist/fonts` (or `$YAIST_FONT_DIR`).
  Inter is packaged in openSUSE (`inter-fonts`); Archivo and Geist Mono are
  not, which is why they are bundled by default.
- egui's default fonts (which carry the Ubuntu Font Licence) are deliberately
  disabled.

**Re-run `scripts/license-report.sh` after every dependency update.** A
re-vendor can add licences the previous one didn't have, so an existing
`License:` tag is not evidence for the next version. Inside an rpmbuild with
the vendored tree, pass `--offline`.

### Suggested spec text

```spec
# Legal-Review-Notice: Rust crates are linked statically. Derived with
# scripts/license-report.sh (cargo tree -e normal -p yaist / -p yaist-gui):
# no copyleft without a permissive alternative (r-efi and self_cell are
# used under MIT/Apache-2.0); OFL-1.1 covers the fonts embedded in yaist-gui.
License:        (MIT OR Apache-2.0) AND Unicode-3.0 AND Zlib
...
%package gui
License:        (MIT OR Apache-2.0) AND ISC AND OFL-1.1 AND Unicode-3.0 AND Zlib
```

## Not part of yAIst: the model and its server

`scripts/llm-server.sh` downloads these on the user's machine. They are not in
the source tarball, the binaries or the package.

| Component | Licence | Source |
|---|---|---|
| llama.cpp server | MIT | `ghcr.io/ggml-org/llama.cpp:server-vulkan` (Ubuntu-based image with its own package licences) |
| Qwen3-4B-Instruct-2507 weights | Apache-2.0 | Qwen team; GGUF conversion by unsloth on Hugging Face |

yAIst talks to any OpenAI-compatible server, so the model is a configuration
choice. Model choices must stay OSI-licensed (Apache-2.0 / MIT); Llama and
Gemma licences do not qualify.

For a distribution, the plan is to replace the download with packages built in
OBS: llama.cpp as an RPM or an openSUSE-based container, and the weights as a
`noarch` data package (e.g. `yaist-model-qwen3-4b`, Apache-2.0) so they get
the same legal review as everything else.
