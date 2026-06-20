# Whistleblower — Logos Basecamp app

A Logos Basecamp GUI for censorship-resistant document publication. The user picks a
file, adds metadata, and submits; the app uploads to Logos Storage, broadcasts the CID +
metadata envelope to the Logos Delivery topic, and optionally anchors on-chain — all via
the `document-indexer` module (linked as a static lib through the C FFI in
`src/document_indexer_ffi.h`).

## Layout

| Path | What |
|---|---|
| `metadata.json` | Basecamp module manifest (`type: basecamp`, deps: storage/delivery/lez_wallet modules, capabilities: `storage.upload`, `delivery.publish`). |
| `qml/UploadPage.qml` | Upload form: file picker + title/description/tags + "anchor on-chain" toggle. |
| `qml/IndexPage.qml` | Browse anchored documents, query-by-CID. |
| `qml/Main.qml` | App shell / navigation. |
| `src/whistleblower_impl.{h,cpp}` | C++ glue calling the `document_indexer` FFI. |
| `src/document_indexer_ffi.h` | C ABI: `document_indexer_upload_and_anchor(...)`, `document_indexer_is_anchored(...)`. |

## Build

1. Build the indexer static lib the app links against:
   ```bash
   cargo build --release -p document-indexer   # produces target/release/libdocument_indexer.a
   ```
2. Build the Basecamp plugin (Qt 6 Quick/Qml). Use the **same Qt 6.9.2 toolchain the Logos
   host links** — a plugin built against a different Qt minor (e.g. 6.11) loads in `lm
   metadata` but fails at runtime with "incompatible Qt library". With the Logos module
   builder's pinned nixpkgs:
   ```bash
   nix build ./basecamp-app#lib \
     --override-input nixpkgs github:NixOS/nixpkgs/e9f00bd893984bc8ce46c895c3bf7cac95331127
   # or, with a local Qt 6.9.2:
   cmake -S basecamp-app -B basecamp-app/build \
     -DQt6_DIR=$QTBASE/lib/cmake/Qt6 && cmake --build basecamp-app/build
   ```
   Output: `whistleblower_plugin` (+ `metadata.json`).

## Load into the Logos app (Basecamp)

Deploy the built plugin + `metadata.json` into the Logos Core modules directory alongside
its declared deps (`storage_module`, `delivery_module`, `lez_wallet_module`), then in the
Logos desktop app open **Basecamp → Whistleblower**. From a headless host you can load it
via the logoscore CLI:

```bash
logoscore -D -m <modules-dir> &           # daemon scans modules at startup
logoscore load-module whistleblower       # auto-loads storage/delivery/wallet deps
```

The `metadata.json` manifest format and the Qt-6.9.2 build recipe are the ones proven to
load in Logos Core for the sibling LP-0008 agent module.

## Note

The GUI build + load requires the Logos Basecamp/Qt runtime (the desktop app or a logoscore
host with the platform modules). The upload→broadcast→anchor logic itself is fully covered
by the standalone `document-indexer` crate and exercised headless by `../demo.sh`.
