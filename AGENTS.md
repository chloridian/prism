Prism is Radia's fork of gpui-ce (`gpui-ce/gpui-ce`). Radia is a Second Life viewer that draws its 3D scene on the same wgpu device as its UI, so Prism renders through `gpui_wgpu` on every platform. The DirectX and Metal renderers are gone, and wgpu is a plain dependency, never a feature. On Windows, `gpui_winit` runs windows and input on winit and has no IME composition yet. The native platform crates for macOS, Linux and the web still handle windows, input, IME, text and clipboard. Prism also carries longbridge's gpui-kit components (`longbridge/gpui-kit` at `b63a46f8`), ported to Prism in `crates/gpui_base`, `crates/gpui_component`, `crates/gpui_component_macros`, `crates/gpui_kit`, `crates/gpui_kit_assets` and the `crates/gpui_story` gallery. Beyond that, Prism carries what Radia needs from its UI framework and merges upstream regularly.

## Radia

- Radia pulls Prism's crates straight from this git repository. Keep upstream's crate names, and take upstream's versions unchanged at each merge.
- Framework code and components any GPUI app could use belong in Prism. Radia keeps what only Radia needs, such as its own widgets and the inspector UI.
- A change is done when Radia builds and runs against it. Prism's own checks passing is not enough.

## Contributing upstream

- Contribute to `gpui-ce/gpui-ce` (pull requests, issues, discussions, comments) only when the owner asks for that specific contribution, and only with changes that help gpui-ce too.
- Build an upstream contribution on a branch from upstream `main` with only that change and no agent files. Follow [AI_POLICY.md](AI_POLICY.md). The owner writes all prose, the pull request discloses LLM help, and tests cover every change.
- Pass `--repo chloridian/prism` to every `gh pr` command. `gh` sends a fork's pull requests to the parent repository by default.

## Diverging from upstream

- Delete upstream code in the same change that makes it dead. That covers native renderers, features that made wgpu optional, generated artifacts nobody reads, and their dependencies. Edit shared upstream files when the cleanup needs it. Merges will conflict sometimes, and a clean tree is worth it.
- Leave alone upstream code that Prism doesn't replace.
- Put new behavior in new modules where practical, and match upstream's code style in files you touch. Radia's `AGENTS.md` rules apply only to Radia.
- Features only Prism has live in crates named `prism*`, and their examples in `examples/prism/`.
- Write the code instead of adding a dependency when writing it is cheap.
- Borrow ideas from WGPUI (`Far-Beyond-Pulsar/WGPUI`), another wgpu-only GPUI fork, when they fit Radia's needs.
- When gpui-ce, gpui-kit and WGPUI offer conflicting versions of a feature, keep the better one and say why in the pull request. If one side is Prism's own work, meaning code the owner built or changed from its source, ask the owner before replacing it. Update the docs that describe the feature in the same change.

## Commits and pull requests

- Commit with one Conventional Commit header and no body: `<type>: <subject>` or `<type>(<scope>): <subject>`. Scopes are optional; include one only when it adds useful context. For crate-level scopes, use the crate name without the `gpui_` prefix (`gpui`, `wgpu`, `windows`, `macos`).
- Name branches with a type prefix and the change (`refactor/wgpu-only-renderer`).
- Title pull requests with a Conventional Commit subject line. Use the title as the squash-merge commit subject. For merge commits, use `merge: <subject>` or `merge(<scope>): <subject>`, including a scope only when it adds useful context. Start the body with the problem in plain prose, then give the solution in a sentence or two. End with a `Verification:` paragraph listing only checks that ran.
- Leave releases to upstream. No tags, no `just publish` or other release recipes, no crates.io publishing.

## Checks

`just` and `nu` aren't installed, so run the justfile's cargo commands directly:

- `cargo fmt --all -- --check`
- `cargo clippy --release --workspace --all-targets -- -D warnings`
- `cargo nextest run --release --workspace --no-fail-fast`, plus `cargo test --release --workspace --doc` when doc examples change
