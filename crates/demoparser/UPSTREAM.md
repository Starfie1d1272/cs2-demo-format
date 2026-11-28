# Upstream `LaihoE/demoparser` Vendor Notes

- **Repository**: https://github.com/LaihoE/demoparser
- **Upstream Commit**: `39942c76b96c02ae714406527a058d81bdb30733` (`main`, post-`v0.42.2`)
- **License**: MIT (see `LICENSE`)

## Local Modifications for Offline Vendoring

1. Removed `crates/csgoproto/build.rs`, `update_protos.py`, `src/main.rs`, `src/parser.rs`, and `prost-build` build-dependency so builds never invoke `git clone` or `protoc` over the network.
2. Kept the pre-generated `protobuf.rs`, `maps.rs`, and `message_type.rs` in `crates/csgoproto/src/`.
3. Removed `crates/demoparser/build.rs` (nested `cargo run`), `[profile.*]` (moved to workspace root `Cargo.toml`), `test_demo.dem`, and `e2e_test.rs` (which required the 60 MB `test_demo.dem` file), and guarded `cut_recording_tests::trailer_after_a_cut_frame_ends_the_demo` to skip when `test_demo.dem` is absent.

## Regenerating Protobuf / Maps Manually

If CS2 protocol definitions need updating in the future:

1. Clone `LaihoE/demoparser` and `GameTracking-CS2` in a temporary directory.
2. Run `csgoproto`'s generator with `protoc` installed.
3. Copy the updated `src/csgoproto/src/{protobuf,maps,message_type}.rs` and `src/parser/src/` into `crates/csgoproto/` and `crates/demoparser/`, keeping `build.rs` and `e2e_test.rs` excluded.
