# Stemma

A git-native editor for scientific papers written in Typst. Writing ideas
map onto git: saving is a commit, a draft is a branch, and a submitted
version is a tag.

## Running

You need `git` and [rustup](https://rustup.rs). The pinned toolchain in
`rust-toolchain.toml` installs itself on the first build.

The UI uses [egui-neo](https://github.com/mehmetmuejde/egui-neo), which
cargo can't fetch yet. Clone it next to this repository:

```sh
git clone git@github.com:mehmetmuejde/egui-neo.git ../egui-neo
```

Then start Stemma:

```sh
cargo run --release -p stemma-app
```

Stemma keeps your papers in a library at `~/Stemma`, which is created on
first start. Each paper is a folder there with its own git repository,
and you can create new papers from the app. To use a different library,
pass its folder:

```sh
cargo run --release -p stemma-app -- path/to/library
```

## Developing

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```
