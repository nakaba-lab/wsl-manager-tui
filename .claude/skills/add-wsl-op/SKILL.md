---
name: add-wsl-op
description: Scaffold a new wsl.exe backend operation end-to-end across the WslBackend trait, the Command/Action message types, the pure reducer, and the runtime dispatch. Use when adding any new interaction with wsl.exe (a new wsl subcommand, an in-distro command, a new lifecycle action).
disable-model-invocation: true
---

# Add a WSL backend operation

A new `wsl.exe` interaction must be threaded through **five** layers in a fixed
order. Skipping any one of them either fails to compile or silently does
nothing. Follow the steps below; keep the `app` layer pure (no IO, no terminal,
no async) — it only *describes* the effect via a `Command`, and the runtime
performs it.

## The five edits

1. **Trait method + real impl — `src/wsl/backend.rs`**
   - Add an `async fn` to the `WslBackend` trait with a doc comment that names
     the exact `wsl.exe` invocation (e.g. `` /// Boot a distro (`wsl -d <name> -- true`). ``).
   - Implement it on `RealWslBackend`. Reuse the existing helpers:
     - `run_wsl(&["..."])` for normal commands (decodes UTF-16LE wsl.exe output).
     - `run_wsl_long(&["..."])` for cancellable long-running ops (sets `kill_on_drop`).
     - For **in-distro** commands (`wsl -d <d> -- ...`) build the `tokio::process::Command`
       directly and decode stdout with `decode_utf8` — NOT `decode_wsl_output`
       (in-distro output is Linux-side UTF-8). See `inner_disk` / `read_conf` for the pattern.
   - Never parse localized STATE text; derive state from `list_running`, not from strings.

2. **Message types — `src/app/message.rs`**
   - Add a `Command` variant describing the effect (carry owned data: `String`, `PathBuf`).
     For lifecycle-style ops, add a `LifecycleOp` variant instead and a `done_message` arm.
   - Add the corresponding result `Action` variant(s): a success carrier and, if the
     result isn't already covered by `OpDone`/`OpFailed`, a typed variant.

3. **Reducer wiring — `src/app/update.rs`**
   - In `update`, handle the key/trigger that should start the op by **returning the
     new `Command`** in the `Vec<Command>` — do not perform IO here.
   - Handle the result `Action` to fold it back into the `Model` (update state,
     clear progress, set status). This is the main unit-test surface — add tests.

4. **Runtime dispatch — `src/runtime/mod.rs`**
   - In `dispatch`, map the new `Command` to a call on `Arc<dyn WslBackend>`, spawn a
     tokio task, and send the resulting `Action` back over the mpsc channel.
   - If the op is long-running/cancellable, follow the Export/Import/Install pattern:
     keep the `AbortHandle` in `current_op` and respect `CancelOp`.
   - If it needs the console (interactive shell), handle it **inline** in `event_loop`
     with `suspend`/`resume` instead of spawning (see `LaunchInlineShell`).

5. **Mock — the test backend**
   - Add the method to `MockBackend` (search tests for `impl WslBackend for MockBackend`)
     so the app can be driven in unit tests without a real WSL host.

## Verify

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings   # warnings are CI errors
cargo test --all                            # reducer tests must cover the new Action
```

Any real-host behavior goes in an `#[ignore]`d test (e.g. `tests/real_wsl.rs`), never
in the default `cargo test` run.
