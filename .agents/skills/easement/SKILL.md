---
name: easement
description: Keeps code inside the architecture declared in easement.yml. Use in a project that has an easement.yml before adding a file, a directory or an import, after changing code, when `easement check` fails, and when easement.yml or its baseline needs to change.
---

# easement

`easement` checks the dependencies between the components of a project against `easement.yml`. The config is the architecture the team chose: change the code to fit it, never the config to fit the code.

If the project has no `easement.yml`, easement is not set up yet: follow "Set up with a coding agent" in the README of <https://github.com/mokevnin/easement>.

## Before writing code

Run `easement explain <path>` for the file or directory you are about to add or change. It prints the component the path belongs to, what that component may depend on and why. Put new code where the imports it needs are allowed.

`easement why <a> <b>` shows the imports, or the chain of imports, behind a dependency between two components.

## After changing code

Run `easement check`. It exits 0 when the code matches the config, 1 on violations and 2 on an error.

- On a branch, `easement check --diff <base>` judges only the lines you added. Run the full `easement check` too when `easement.yml` or the baseline changed, because a rule change touches no import line.
- `--format json` gives the same report as data. When `confidence` is `partial`, a pass covers only the code an indexer read: say so, and name the files in `stats.uncovered`. When `cached` is set, the indexes came from the cache; after changing dependencies outside the project without a lock file, run `easement check --no-cache`.
- On exit 2, read the message. For a missing indexer or missing dependencies, run `easement doctor`.
- `easement impact --diff <base>` lists every file that depends on your change: run the tests among them. When it says `Partial`, files no indexer read or files the change deletes may hide more. `Cached:` means the indexes came from the cache; `--no-cache` indexes again.

## When the check fails

A violation reads like `internal/handlers/books.go:6: handlers -> repository is not allowed (imports internal/repository/books.go) [dependency]`, followed by `because:` with the reason of the rule.

1. Read `because`: it says where the code should go instead.
2. Fix the code: go through the component the rule allows (call the service, not the repository), move the file into the component it belongs to, or turn the dependency around. A `reach` violation lists a chain of imports; remove any one hop of it.
3. Do not edit `easement.yml`, regenerate the baseline or add an `easement:disable` comment to get the check through. If the rule itself looks wrong, stop and ask a human (the `owner` printed with the violation, when there is one), with the violation and why you think the rule is wrong.

Two cases are cleanup, not exceptions:

- **Stale baseline entries**: a violation recorded in the baseline is gone. Run `easement baseline` to drop them. It never writes new violations: it lists them and exits 1, so fix those as above. Legalizing new debt with `easement baseline --accept-new` needs a human.
- **`unused-suppression`**: the excused import is gone, so delete the comment.

Warnings (rules in `warn`) do not fail the check, but they are debt the team tolerates for now: add no new ones, and fix the ones in code you change.

An `unused` violation means no chain of imports from an `entry` reaches the file. Delete it only if your own change added it; otherwise tell a human, because a framework or reflection may load it, and the fix is then an `entry`.

An `excluded` violation means the code imports a file that `exclude` hides, so no rule sees the dependency. Import from a checked component instead; if the file is generated code the project needs, ask a human to move its pattern to `target_only`.

An `unmatched` violation is about the config, not the code: an `in` pattern of a component points at no file, usually because the code moved. It needs a change to `easement.yml`, so tell a human which pattern it is and where the code went.

## Changing easement.yml

Change the config only when a human asks for it or agrees to it.

- A new directory that belongs to no component: add a component with `in`, and a rule with `may_depend_on` and `because`. With `strict: true` such a directory fails as `orphan` until it has a component or is listed in `unowned`.
- Every rule gets a `because` that tells the reader where the code should go instead.
- `easement suggest` lists the allowances no import uses and the imports nothing allows. `easement suggest --patch | git apply` drops the unused allowances and keeps comments and reasons. Leave alone the entries it lists apart as unreliable: they sit next to files no indexer covered.
- Run the full `easement check` after every change to the config.

`easement schema` prints the full format of `easement.yml`; the README describes every rule.
