# Contribute

## Pre-commit setup

Install pre-commit hooks:

```shell
pre-commit install
```

To run all pre-commit hooks without making a commit run:

```shell
pre-commit run --all-files
```

To create a commit without running the pre-commit hooks run:

```shell
git commit --no-verify
```

## Build documentation

To build the documentation for the project run:

```shell
cargo doc --no-deps --document-private-items --verbose --open --color always --release --target-dir ./docs
```
