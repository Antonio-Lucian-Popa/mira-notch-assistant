# Contributing to Mira

Thanks for your interest! Mira is built in small, always-compiling steps, and
contributions are welcome.

## Workflow (fork & pull request)

`main` is protected — **nobody pushes to it directly**. All changes land through
a pull request that a maintainer reviews and merges.

1. **Fork** this repository to your own account.
2. Create a branch in your fork: `git checkout -b my-change`.
3. Make your change, keeping the surrounding style (comments in **English**).
4. Make sure it builds and the tests pass:
   ```sh
   cargo test -p mira-core
   cargo clippy --all-targets
   cargo fmt --check
   ```
   For the GNOME extension, verify it loads without JS errors (a headless
   `gnome-shell --headless --virtual-monitor 1280x720` run is handy).
5. Push to your fork and **open a pull request** against `main`.
6. A maintainer reviews and merges. Please keep PRs focused and describe what
   changed and why.

## Commit messages

Use clear, descriptive messages (e.g. `feat(gnome): …`, `fix(core): …`,
`docs: …`). Avoid noise commits — squash locally if needed.

## Code of conduct

Be kind and constructive. Harassment or hostility isn't welcome here.

## License

By contributing, you agree that your contributions are licensed under the
project's [MIT License](LICENSE).
