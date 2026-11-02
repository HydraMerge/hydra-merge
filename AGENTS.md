# Agent notes

## heavy

Tests, Clippy and release builds take a machine-wide slot. Run them through this Makefile (`make test`, `make clippy`, `make build`). A one-off command is `heavy -- <command>`.

`heavy` is installed with `brew install mokevnin/tap/heavy`. Screenshot tests (`ui_shots`, `docs_shots`) stay unwrapped: they need a GPU and are not part of the default check.

## easement

`easement.yml` is the architecture. Use the easement skill before adding a file or an import, and whenever `easement check` fails.

After changing code, run `easement check`. It must exit 0.

Do not edit `easement.yml`, regenerate the baseline or add an `easement:disable` comment unless a human asks.

`src/lib.rs` only declares modules and stays outside the components on purpose. `git`, `discover`, `trust` and `model` import each other, so they are one `domain` component. The rule that holds is that `domain` does not import `ui`.
