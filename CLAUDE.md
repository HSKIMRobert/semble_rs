# semble_rs Agent Instructions

Use `semble_rs` to keep code exploration and build logs small.

## Exploration Flow

Prefer the smallest useful command.

```bash
semble_rs tree . --symbols                     # step 0: codebase map (replaces ls -R, 4×–747× cheaper)
semble_rs plan "<task>" . -k 5                 # optional 0.5: ambiguous task / new repo
semble_rs search "<feature>" . --outline -k 8  # pass 1: structural overview
semble_rs search "<feature-or-symbol>" . --compact -k 8
semble_rs deps <file> .
semble_rs impact <file> .
```

Start with `tree --symbols` on a new repo — gitignore-aware, ~150 ms. Use `plan` when the starting point is still unclear. Treat `Confidence: low` candidates as leads, not facts. If the feature or symbol is already known, skip `tree`/`plan` and go straight to `search --outline` or `search --compact`.

`tree` options: `-d` (dirs only), `--max-depth N`, `--symbols` (top-level fn / struct / class), `--lang rust,python` (filter).

## Token Discipline

- Do not start by opening whole files.
- Do not use `--json` or chunk bodies unless compact results are insufficient.
- Search with natural-language feature descriptions before guessing symbol names.
- Pass a directory path to search commands, not a single file path.
- Use `deps` and `impact` before editing shared or central files.
- Use `--model <repo-or-path>` (or `SEMBLE_MODEL_PATH` env) to override the default embedder per-call. Default: `minishlab/potion-code-16M`.
- Fall back to raw `grep`, `cat`, `find`, or `ls` only when `semble_rs` is insufficient.

## Build And Test Output

Always pipe noisy commands through `semble_rs digest`.

```bash
cargo build 2>&1   | semble_rs digest
cargo test 2>&1    | semble_rs digest
pnpm install 2>&1  | semble_rs digest
pytest 2>&1        | semble_rs digest
gradle test 2>&1   | semble_rs digest
tsc --noEmit 2>&1  | semble_rs digest
go test ./... 2>&1 | semble_rs digest
ruff check . 2>&1  | semble_rs digest
mypy . 2>&1        | semble_rs digest
make 2>&1          | semble_rs digest
swift build 2>&1   | semble_rs digest
gh run view <id> --log-failed | semble_rs digest
```

`digest` keeps errors, file locations, tracebacks, panic stacks, and failed CI bodies. It only collapses progress noise.

## Reporting

When summarizing work, keep it short:

- files changed
- key behavior change
- verification command
- remaining risk or low-confidence area

Do not quote fixed whole-session savings unless a workflow benchmark was run. It is okay to cite measured command-level savings, such as `digest` fixture results or byte counts from `wc -c`.
