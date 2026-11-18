Prism is a fork of gpui-ce (`gpui-ce/gpui-ce`) maintained for Radia, a Second Life viewer. Its goal is a GPUI that renders only through wgpu on every platform, so Radia can draw its 3D scene on the same wgpu device as its UI. `gpui_wgpu` is the one renderer: the DirectX and Metal renderers are gone, and wgpu is never behind a feature. The native platform crates for Windows, macOS, Linux and the web stay for windows, input, IME, text and clipboard. Prism also carries the other changes Radia needs from its UI framework, and otherwise follows upstream, which it merges regularly.

## Upstream

- Open pull requests, issues, discussions or comments on `gpui-ce/gpui-ce` only when the owner asks for that specific contribution.
- Prism's own pull requests target `hymenaei/Prism`. `gh` defaults a fork's pull requests to the parent repository, so always pass `--repo hymenaei/Prism`.
- An upstream contribution must also benefit gpui-ce. Build it on a branch from upstream `main` that carries only that change, with no agent files, and follow [AI_POLICY.md](AI_POLICY.md): the owner writes all prose, LLM assistance is disclosed, and tests cover every change.

## Compatibility with Radia

- Radia replaces gpui-ce with Prism through `[patch.crates-io]`, so its crates.io dependents, such as `gpui_ce_components`, build against Prism. Keep every crate name (`gpui-ce`, `gpui_ce_*`) and version equal to the gpui-ce release Radia uses. A renamed crate or a changed version silently drops the patch.
- A change is done when Radia builds and runs against it, not only when Prism's own checks pass.

## Changes

- Delete upstream code that Prism replaces, such as a native renderer or a feature that made wgpu optional, rather than leaving it unused. A deleted file costs only a trivial conflict on merge.
- Every edited upstream line costs a real conflict on some future merge. Keep edits to shared upstream files, such as `window.rs`, `platform.rs` and the manifests, as small as the change allows, and leave upstream code Prism does not replace untouched.
- Put new behavior in new modules where practical, and match the surrounding upstream style in files you touch. When a feature exists in WGPUI (`Far-Beyond-Pulsar/WGPUI`), study its design before writing your own.
- Follow upstream conventions in this repository. Radia's `AGENTS.md` rules apply only to Radia.
- Commit with one Conventional Commit subject line, `<type>(<scope>): <subject>`, and no body. The scope is the crate without its `gpui_` prefix (`gpui`, `wgpu`, `windows`, `macos`); omit it only for repository-wide changes.
- Name branches by type prefix and change (`refactor/wgpu-only-renderer`).
- Title pull requests with an imperative sentence in sentence case and no type prefix. Open the body with the problem in plain prose, then the solution in a sentence or two, and end with a `Verification:` paragraph that reports only checks that ran.
- Leave releases to upstream: no tags, no `just publish` or release recipes, no crates.io publishing.

## Checks

`just` and `nu` are not installed. Run the justfile's cargo commands directly:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
