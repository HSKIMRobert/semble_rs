<!--
Keywords: code search, semantic code search, AI agent, LLM, BM25, embeddings,
          tree-sitter, AST, dependency graph, impact analysis, Rust, CLI,
          Claude Code, Codex, Cursor, grep replacement, token reduction,
          potion-code, model2vec, hybrid search, RRF, build output digest,
          CI log compression, korean code search, 한글 코드 검색
-->

# semble_rs

> **Fast, AI-agent-native code search + build/test/CI output compression — written in Rust.**
> One hybrid (BM25 + semantic) search replaces `grep`/`cat`/`read`/`ls`;
> `semble_rs digest` collapses 3 MB CI logs into 35 KB.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-blue.svg)](#installation)
[![Token savings](https://img.shields.io/badge/agent%20tokens-up%20to%20--98.9%25-brightgreen.svg)](#digest--buildtestci-output)
[![한국어](https://img.shields.io/badge/한국어-README.ko.md-blue.svg)](./README.ko.md)

**Keywords:** AI code search · LLM agent tools · grep/cat replacement · BM25 + embeddings · Tree-sitter AST · build / CI log compression · Rust CLI · Claude Code · Codex · Cursor · 한글 코드 검색

한국어 사용자는 [README.ko.md](./README.ko.md)를 참고하세요.

---

## What it does

AI agents burn tokens two ways:
1. **Exploring code** — repeated `grep` → `cat` → `read` reads megabytes of irrelevant content.
2. **Reading build / CI output** — `cargo build`, `pnpm install`, `gh run view --log` dump tens of KB to MB of progress noise.

`semble_rs` collapses both:

| Stage | Without `semble_rs` | With `semble_rs` | Savings |
|---|---|---|---|
| Code lookup | `ls` → `grep` → `cat file₁` → `cat file₂` → … | `semble_rs search "auth flow" . --outline` | large reduction in exploratory reads |
| CI failure debug | `gh run view <id> --log-failed` (3.3 MB raw) | `gh run view … \| semble_rs digest` (35 KB) | **-98.9%** |

It is a single Rust binary, no runtime dependencies, with a Rust rewrite of [MinishLab/semble](https://github.com/MinishLab/semble) at its core plus dependency graphs, AST chunking, Korean/CJK Unicode search, and an output-digest pipeline.

## Installation

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
git clone https://github.com/johunsang/semble_rs.git && cd semble_rs
cargo install --path .
```

The binary lands at `~/.cargo/bin/semble_rs`. On first run, the default embedding model `minishlab/potion-code-16M` (~60 MB) is downloaded from HuggingFace.

## Quick start

```bash
# Code search (replaces grep / cat / read / ls)
semble_rs plan "fix auth flow bug" ./my-project -k 5       # optional: recommend a minimal exploration flow
semble_rs search "auth flow" ./my-project --outline      # pass 1: structural overview
semble_rs search "loginWithEmail" ./my-project --compact # pass 2: matching lines

# Dependencies and change-impact
semble_rs deps   src/lib/auth.ts ./my-project
semble_rs impact src/lib/auth.ts ./my-project

# Build / test / CI output compression
cargo build 2>&1     | semble_rs digest
pnpm install 2>&1    | semble_rs digest
gh run view <id> --log-failed | semble_rs digest
```

## Search — output modes

| Mode | Output | Token cost vs `--compact` | When to use |
|---|---|---|---|
| `--outline` | One signature line per chunk | **-47%** | First-pass structural scan |
| `--group` | Directory grouping + match lines capped at 3 (`+N` overflow) | -47% | Many match lines per chunk |
| `--compact` | Score + path + every matching line | baseline | Precision scan |
| `--json --strip` | Chunk bodies (comments stripped) | +800% | Tooling / pipeline integration |
| `--json` | Chunk bodies (raw) | +900% | Tooling / pipeline integration |

**Recommended:** `--outline` to overview → `--compact` to narrow → `--json --strip` only if the chunk body itself is needed.

For agent sessions, `semble_rs plan "<task>" /path -k 5` can be used before the first search. It runs a small search, ranks likely files/chunks, and prints the next `--outline`, `--group`, `--compact`, `deps`, and `impact` commands so the agent does not wander into full-file reads.

`plan` is a guardrail, not an oracle: it is useful for ambiguous tasks, new repositories, and “where should I start?” moments. If it reports low confidence, treat candidates as leads and broaden the natural-language query. If you already know the feature or symbol, go straight to `search --outline` / `search --compact`.

**Savings note:** individual commands are measured directly (`--outline` is often much smaller than default search output, and `digest` reaches 98.9% on the GitHub Actions fixture above). Whole-session savings depend on whether the agent actually avoids full-file reads and raw logs, so they should be benchmarked per workflow rather than quoted as a fixed number.

`--outline` accuracy on the 33-query self-benchmark: **100% well-formed** signatures (parens balanced, no truncation).

**Performance** (measured): the index is rebuilt every run (no persistent cache). Search and `plan` complete in roughly:

| Repo size (code files) | search / plan |
|---|---|
| 22 (this repo) | ~0.15 s |
| 57–120 | ~0.3–0.7 s |
| 1,600 | ~10 s |

`digest` is independent of repo size and processes a 3.3 MB CI log in ~20 ms.

**100-query benchmark** on this repo (v0.5.0, default model): **R@5 97%, R@10 100%, MRR 0.84**, median 163 ms / query. Per-category breakdown and every miss explained in [`docs/benchmark_100.md`](./docs/benchmark_100.md). The query set is in [`docs/eval_set_100.json`](./docs/eval_set_100.json).

## `digest` — build / test / CI output

Auto-detects and compresses output from common toolchains. Errors and failures are never lost — only progress lines collapse to counts.

**Supported handlers:** `cargo`, `pnpm`/`npm`/`yarn`/`bun`, `tsc`, `pytest`, `go test`, `gradle`, `ruff`, `mypy`, `clang`/`gcc`/`cmake`/`make`/`swiftc` (`compiler`), GitHub Actions (`ci`). Unknown formats pass through unchanged.

Measurements on 15 real-world fixtures:

| Fixture | Raw → digest | Savings |
|---|---|---|
| `cargo build` (clean, 218 crates) | 7,611 B → 59 B | **-99.2%** |
| `cargo test` (45 passing) | 3,368 B → 369 B | -89.0% |
| `pnpm install` | 1,323 B → 349 B | -73.6% |
| `tsc` (13 errors, 5 codes) | 1,085 B → 648 B | -40.3% |
| `pytest` (4 failures) | 2,762 B → 2,330 B | -15.6% |
| **GitHub Actions log (rust-lang/rust failed CI, real)** | **3.3 MB → 35 KB** | **-98.9%** ⭐ |
| `go test` (with panic + stack) | 1,034 B → 475 B | -54.1% |
| `gradle test` (2 failures) | 1,232 B → 522 B | -57.6% |
| `ruff` (9 violations, 3 codes) | 624 B → 597 B | -4.3% |
| `mypy` | 336 B → 237 B | -29.5% |
| `clang`/`cmake`/`swift` compilers | ~600 B (progress stripped) | -3 ~ -30% |
| **TOTAL (15 fixtures)** | **3.33 MB → 43 KB** | **-98.7%** |

```bash
# Force a specific handler when auto-detect misses
semble_rs digest --format ci  ci_log.txt
semble_rs digest --format gradle gradle_test.log

# Inspect which handler was picked
semble_rs digest --show-format my_log.txt
```

**Preservation guarantees**
- File:line:col, traceback, panic stack, failed-step bodies are always kept.
- Repeated error codes are grouped (e.g. `TS2322` × 9 → top 3 + `+6 more`).
- CI `##[group]` blocks: successful groups collapse to one line; failed groups keep their trailing 80 lines verbatim.

## How does `semble_rs` compare to existing tools?

| Need | `ripgrep` | `ast-grep` | IDE/agent built-in search (Cursor, etc.) | **`semble_rs`** |
|---|---|---|---|---|
| Exact symbol / regex grep | **★ fastest** | ✓ via patterns | ✓ | ✓ (hybrid) |
| Natural-language query ("where do we strip ANSI") | ✗ | ✗ | partial | **★** |
| AST structural patterns (`fn $name($$$)`) | ✗ | **★** | ✗ | wraps ast-grep via `find-pattern` |
| Dependency graph + impact analysis | ✗ | ✗ | partial | **★** |
| Build / CI log compression | ✗ | ✗ | ✗ | **★ `digest`** |
| Single binary, no daemon | ✓ | ✓ | ✗ (IDE bound) | ✓ |
| Korean / CJK indexing | ✓ (byte grep) | partial | varies | ✓ (Unicode BM25) |

`semble_rs` is not a replacement for `ripgrep` when you already know the exact symbol — `ripgrep` is unbeatably fast at raw text. It is a replacement for **the pattern of "agent runs grep → reads several files → repeats"** by collapsing that loop into one hybrid-search call, and adds:

- **`digest`** — collapses 3 MB CI logs to 35 KB (no other tool does this in a generic way),
- **`deps` / `impact`** — file-level dependency graph with `--dot` Graphviz output (use `… --dot | dot -Tpng > graph.png`),
- **`plan`** — recommends a token-efficient exploration flow when the agent doesn't know where to start,
- **`find-pattern`** — thin wrapper around `ast-grep` for structural queries (requires `ast-grep` installed).

Use **`ripgrep`** when you have the exact symbol and just want to list call sites. Use **`ast-grep`** (or `semble_rs find-pattern`) when you need a structural code rewrite. Use **`semble_rs`** when the agent is exploring, the symbol name is unknown, or you want CI/build logs compressed.

## Supported languages (search)

| Language | Search | AST chunking | Dependency graph |
| --- | --- | --- | --- |
| Rust | ✓ | ✓ | ✓ |
| Python | ✓ | ✓ | ✓ |
| JavaScript | ✓ | ✓ | ✓ |
| TypeScript | ✓ | ✓ | ✓ |
| Go | ✓ | ✓ | ✓ |
| Java | ✓ | ✓ | ✓ |
| C / C++ | ✓ | ✓ | ✓ |
| **Kotlin** (v0.3.0+) | ✓ | ✓ | ✓ |
| Ruby, PHP, Swift, others | ✓ | line-based fallback | — |
| **HTML, CSS, SCSS, Vue, Svelte** (v0.6.0+) | ✓ | line-based fallback | — |

## Integration with AI coding agents

### Global `CLAUDE.md` (Claude Code) and `AGENTS.md` (Codex)

Drop a section like the following into `~/.claude/CLAUDE.md` and `~/.codex/AGENTS.md`:

```markdown
# semble_rs — replaces grep, cat, read, ls + compresses build/CI output

ALWAYS use these instead of raw grep/cat/find/read:

  semble_rs plan   "<task>"    /path             # optional 0: plan + candidate files
  semble_rs search "<feature>" /path --outline      # 1단계 탐색
  semble_rs search "<symbol>"  /path --compact      # 2단계 정밀 탐색
  semble_rs deps   <file>      /path
  semble_rs impact <file>      /path

ALWAYS pipe build / test / CI output through `semble_rs digest`:

  cargo build 2>&1   | semble_rs digest
  pnpm install 2>&1  | semble_rs digest
  pytest 2>&1        | semble_rs digest
  gradle test 2>&1   | semble_rs digest
  gh run view <id> --log-failed | semble_rs digest

Rules: never guess symbol names (use natural-language descriptions instead),
always pass a directory path (not a file path), and only fall back to `grep`
when semble_rs results are insufficient. Treat low-confidence `plan` output as
leads, not facts. Do not use `--json` or open whole files unless the compact
results are not enough.
```

### Per-project (any agent)

Same content in a project-root `CLAUDE.md` or `AGENTS.md` works for Claude Code, Codex, Cursor (`.cursorrules`), Aider, and OpenHands.

## License

MIT — see [LICENSE](./LICENSE).

Credits:
- Upstream: [MinishLab/semble](https://github.com/MinishLab/semble) — the Python implementation this Rust port draws from.
- Embedding model: [`minishlab/potion-code-16M`](https://huggingface.co/minishlab/potion-code-16M).
- Built on: `tree-sitter`, `ndarray`, `safetensors`, `tokenizers`, `hf-hub`, `ignore`, `clap`.
