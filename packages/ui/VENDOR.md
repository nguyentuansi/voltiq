# Vendored: `@landing-v/ui`

This directory is a **vendored copy** of `packages/ui` from the `landing-v` monorepo.

- Upstream: https://github.com/nguyentuansi/landing-v (`packages/ui`)
- Vendored at commit: `5ec6b766c24dbd8ef47704702fee817caf82ccbe`
- Consumed as **source** (Svelte 5 components, zero runtime deps). `@voltiq/dashboard`
  imports it as `@landing-v/ui` via the pnpm workspace.

`@landing-v/ui` is a private workspace package (not its own git repo), so a git submodule
of just this folder isn't possible. The sync alternative is to add the whole `landing-v`
repo as a submodule under `vendor/` and point the pnpm workspace at
`vendor/landing-v/packages/ui`. We vendor instead to keep the tool self-contained.

To refresh: re-copy `packages/ui` from a newer landing-v checkout and bump the commit above.
