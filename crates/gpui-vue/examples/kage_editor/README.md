# Kage Editor

Kage Editor is a native macOS editor for creating, inspecting, and exporting KAGE glyph stroke data. Its interface is built with `gpui-vue`.

The Components panel is populated with 50 randomized live GlyphWiki results on
launch and can refresh the batch. Component KAGE data and its recursive
dependencies are fetched only when selected; the application does not ship an
approximate fallback component library.

## Build the macOS app bundle

Install [`cargo-bundle`](https://github.com/burtonageo/cargo-bundle), then run this command from the repository root:

```sh
cargo bundle --release --manifest-path crates/gpui-vue/examples/kage_editor/Cargo.toml
```

The generated `.app` bundle is written below `target/release/bundle/osx/`. Packaging uses `assets/KageEditor.png` and `assets/KageEditor@2x.png` for the application icon.
