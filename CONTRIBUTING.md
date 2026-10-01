# Contributing to CueLine

Thanks for helping! CueLine is MIT-licensed, and contributions are accepted under the same licence.

## Ground rules

* **Timing first.** Anything that runs on the audio thread (`engine/mixer.rs` and the
  callback in `engine/device.rs`) must not allocate, lock, block or log. If you need to send data
  to it, use the command queue or an atomic.
* Timecode maths belongs in `cueline-core`, with tests. The LTC round-trip tests and the MTC
  timing test must keep passing.
* Keep the executable small. Talk to us in an issue before adding a heavy dependency.

## Workflow

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three commands on Windows. Commit messages follow
[Conventional Commits](https://www.conventionalcommits.org) (`feat:`, `fix:`, `docs:` …).

## Screenshots

`CUELINE_SCREENSHOT=out.ppm cargo run -- path/to/show.cueline` renders the window to a file and
quits. Add `CUELINE_SCREENSHOT_PLAY=5` to play for five seconds first, or
`CUELINE_SCREENSHOT_WINDOW=prefs` to open a dialog.
