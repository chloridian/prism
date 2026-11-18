Prism is Radia's fork of gpui-ce (`gpui-ce/gpui-ce`). Radia is a Second Life viewer that draws its 3D scene on the same wgpu device as its UI, so Prism renders through `gpui_wgpu` on every platform. The DirectX and Metal renderers are gone, and wgpu is a plain dependency, never a feature. On Windows, `gpui_winit` runs windows and input on winit and has no IME composition yet. The native platform crates for macOS, Linux and the web still handle windows, input, IME, text and clipboard. Beyond that, Prism carries what Radia needs from its UI framework and merges upstream regularly.

## Radia

- Radia pulls Prism's crates straight from this git repository. Keep upstream's crate names, and take upstream's versions unchanged at each merge.
- Framework code and components any GPUI app could use belong in Prism. Radia keeps what only Radia needs, such as its own widgets and the inspector UI.
- A change is done when Radia builds and runs against it. Prism's own checks passing is not enough.

## Contributing upstream

- Contribute to `gpui-ce/gpui-ce` (pull requests, issues, discussions, comments) only when the owner asks for that specific contribution, and only with changes that help gpui-ce too.
- Build an upstream contribution on a branch from upstream `main` with only that change and no agent files. Follow [AI_POLICY.md](AI_POLICY.md). The owner writes all prose, the pull request discloses LLM help, and tests cover every change.
- Pass `--repo hymenaei/Prism` to every `gh pr` command. `gh` sends a fork's pull requests to the parent repository by default.

## Diverging from upstream

- Delete upstream code in the same change that makes it dead. That covers native renderers, features that made wgpu optional, generated artifacts nobody reads, and their dependencies. Edit shared upstream files when the cleanup needs it. Merges will conflict sometimes, and a clean tree is worth it.
- Leave alone upstream code that Prism doesn't replace.
- Put new behavior in new modules where practical, and match upstream's code style in files you touch. Radia's `AGENTS.md` rules apply only to Radia.
- Borrow ideas from WGPUI (`Far-Beyond-Pulsar/WGPUI`), another wgpu-only GPUI fork, when they fit Radia's needs.
- When gpui-ce, gpui-kit and WGPUI offer conflicting versions of a feature, keep the better one and say why in the pull request. If one side is Prism's own work, meaning code the owner built or changed from its source, ask the owner before replacing it. Update the docs that describe the feature in the same change.

## Commits and pull requests

- Commit with one Conventional Commit subject line, `<type>(<scope>): <subject>`, and no body. The scope is the crate name minus `gpui_` (`gpui`, `wgpu`, `windows`, `macos`). Drop the scope only for repository-wide changes.
- Name branches with a type prefix and the change (`refactor/wgpu-only-renderer`).
- Title pull requests with an imperative sentence in sentence case, without a type prefix. Start the body with the problem in plain prose, then give the solution in a sentence or two. End with a `Verification:` paragraph listing only checks that ran.
- Leave releases to upstream. No tags, no `just publish` or other release recipes, no crates.io publishing.

## Checks

`just` and `nu` aren't installed, so run the justfile's cargo commands directly:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
