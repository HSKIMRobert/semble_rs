# semble_rs

원본 [semble](https://github.com/MinishLab/semble) (Python)을 Rust로 재작성하고, 의존성 분석 기능을 추가한 프로젝트.

## 원본 semble이란

MinishLab이 만든 AI 에이전트용 코드 검색 라이브러리. 에이전트가 `grep` + `read`로 파일을 하나씩 읽는 대신, 자연어 쿼리 한 줄로 관련 코드 스니펫만 반환합니다.

**원본의 핵심 구조:**

```
쿼리 → BM25(키워드) + Semantic(임베딩) → RRF 하이브리드 융합 → 스마트 랭킹 → 결과
```

**원본의 특징:**

- Python (model2vec + bm25s + vicinity)
- potion-code-16M 임베딩 모델
- MCP 서버 모드 지원
- 줄 기반 청킹
- \~98% 토큰 절감 (파일 전체 읽기 대비)

## 원본의 단점과 개선 사항

```
원본 semble (Python)                       semble_rs (Rust)
┌──────────────────────────┐               ┌──────────────────────────────────┐
│                          │               │                                  │
│  검색만 가능             │   ──────►     │  검색 + 의존성 분석 + 영향 분석  │
│  (search, find-related)  │               │  (search, deps, impact)          │
│                          │               │                                  │
├──────────────────────────┤               ├──────────────────────────────────┤
│                          │               │                                  │
│  줄 기반 청킹            │   ──────►     │  Tree-sitter AST 기반 청킹       │
│  (고정 길이로 잘라서     │               │  (함수/클래스/구조체 단위 분할)   │
│   함수가 중간에 잘림)    │               │                                  │
│                          │               │                                  │
├──────────────────────────┤               ├──────────────────────────────────┤
│                          │               │                                  │
│  줄번호 없음             │   ──────►     │  grep 내장 — 매칭 줄번호 + 코드  │
│  (청크 범위만 반환)      │               │  라인을 결과에 포함              │
│                          │               │                                  │
├──────────────────────────┤               ├──────────────────────────────────┤
│                          │               │                                  │
│  긴 파일 누락            │   ──────►     │  형제 청크 부스팅으로 해결       │
│  (819줄 파일의 함수를    │               │  (같은 파일의 다른 청크도        │
│   검색에서 놓침)         │               │   키워드 매칭 시 점수 올림)      │
│                          │               │                                  │
├──────────────────────────┤               ├──────────────────────────────────┤
│                          │               │                                  │
│  노이즈 제거 없음        │   ──────►     │  점수 필터 — 1위 점수의 12%      │
│  (항상 k개 전부 반환)    │               │  미만 결과 자동 제거             │
│                          │               │                                  │
├──────────────────────────┤               ├──────────────────────────────────┤
│                          │               │                                  │
│  Python (느림)           │   ──────►     │  Rust (빠름)                     │
│  pip install 필요        │               │  단일 바이너리, 의존성 없음      │
│                          │               │                                  │
└──────────────────────────┘               └──────────────────────────────────┘
```

## 아이디어 도식

```
                          ┌─────────────────────────────────────────┐
                          │            semble_rs 검색 파이프라인     │
                          └─────────────────────────────────────────┘

  ┌─────────┐     ┌──────────────────────────────────────────────────────────────┐
  │  소스   │     │                    인덱싱 (한 번)                            │
  │  코드   │────►│                                                              │
  │  파일   │     │  ┌──────────────┐  ┌──────────────┐  ┌────────────────────┐  │
  └─────────┘     │  │ Tree-sitter  │  │   임베딩     │  │   의존성 그래프    │  │
                  │  │ AST 파싱     │  │   인코딩     │  │   구축            │  │
                  │  │              │  │              │  │                    │  │
                  │  │ 함수/클래스  │  │ potion-code  │  │ import 추출       │  │
                  │  │ 단위 청킹   │  │ -16M 모델    │  │ 심볼 추출         │  │
                  │  │              │  │              │  │ 파일간 의존 해석  │  │
                  │  └──────┬───────┘  └──────┬───────┘  └────────┬───────────┘  │
                  │         │                 │                   │              │
                  │         ▼                 ▼                   ▼              │
                  │  ┌──────────────┐  ┌──────────────┐  ┌────────────────────┐  │
                  │  │  BM25 인덱스 │  │ 벡터 인덱스  │  │  파일 그래프       │  │
                  │  │  (키워드)    │  │ (시맨틱)     │  │  (deps/impact)     │  │
                  │  └──────────────┘  └──────────────┘  └────────────────────┘  │
                  └──────────────────────────────────────────────────────────────┘

  ┌─────────┐     ┌──────────────────────────────────────────────────────────────┐
  │         │     │                    검색 (매 쿼리)                            │
  │  쿼리   │────►│                                                              │
  │         │     │  ┌──────────┐  ┌──────────┐                                  │
  └─────────┘     │  │  BM25    │  │ Semantic │                                  │
                  │  │  점수    │  │  점수    │                                  │
                  │  └────┬─────┘  └────┬─────┘                                  │
                  │       │             │                                         │
                  │       ▼             ▼                                         │
                  │  ┌────────────────────────┐                                   │
                  │  │    RRF 하이브리드 융합  │                                   │
                  │  │    (alpha 가중 결합)    │                                   │
                  │  └───────────┬────────────┘                                   │
                  │              │                                                │
                  │              ▼                                                │
                  │  ┌────────────────────────┐                                   │
                  │  │    스마트 랭킹          │                                   │
                  │  │    · 정의 부스팅        │                                   │
                  │  │    · 파일 일관성 부스팅  │                                   │
                  │  │    · 형제 청크 부스팅    │  ◄── 긴 파일 누락 해결           │
                  │  │    · 그래프 부스팅       │  ◄── 의존 파일도 검색 반영       │
                  │  │    · 테스트 파일 페널티  │                                   │
                  │  └───────────┬────────────┘                                   │
                  │              │                                                │
                  │              ▼                                                │
                  │  ┌────────────────────────┐                                   │
                  │  │    노이즈 필터          │  ◄── 1위의 12% 미만 제거         │
                  │  └───────────┬────────────┘                                   │
                  │              │                                                │
                  │              ▼                                                │
                  │  ┌────────────────────────┐                                   │
                  │  │    매칭 라인 추출       │  ◄── grep 내장                   │
                  │  │    (줄번호 + 코드)      │                                   │
                  │  └───────────┬────────────┘                                   │
                  │              │                                                │
                  └──────────────┼────────────────────────────────────────────────┘
                               │
                               ▼
                  ┌──────────────────────────┐
                  │  결과                     │
                  │                          │
                  │  ## 1. src/lib/auth.ts    │
                  │    L45: function login()  │
                  │    L89: auth.signIn()     │
                  │                          │
                  │  ## 2. src/api/route.ts   │
                  │    L12: import { login }  │
                  └──────────────────────────┘


                          ┌─────────────────────────────────────────┐
                          │          의존성 분석 (원본에 없는 기능)   │
                          └─────────────────────────────────────────┘

  semble_rs deps src/lib/firestore.ts .

      firestore.ts
      ├── 정의: upsertUser, getUser, createPage, deletePage ...
      ├── 의존: firebase.ts
      └── 사용처: [slug]/page.tsx, api/pages/route.ts, dashboard/page.tsx ...

  semble_rs impact src/lib/firestore.ts .

      firestore.ts 변경 시 영향:
      ├── 직접 import: api/pages/route.ts, dashboard/page.tsx ...
      └── 간접 전이: api/admin/route.ts (dashboard → admin)
```

## 왜 Rust인가

원본은 Python입니다. Rust로 재작성한 이유:

```
Python 런타임                              Rust 바이너리
┌──────────────────────────┐               ┌──────────────────────────────┐
│                          │               │                              │
│  pip install 필요        │               │  단일 바이너리 (~15MB)       │
│  Python 3.10+ 필수       │   ──────►     │  런타임 의존성 없음          │
│  venv 권장               │               │  어디서든 바로 실행          │
│                          │               │                              │
├──────────────────────────┤               ├──────────────────────────────┤
│                          │               │                              │
│  model2vec (PyTorch)     │               │  ndarray + safetensors       │
│  bm25s (NumPy)           │   ──────►     │  직접 구현 (외부 ML 없음)   │
│  vicinity (NumPy)        │               │  순수 Rust 행렬 연산        │
│  ~200MB+ 의존성          │               │                              │
│                          │               │                              │
├──────────────────────────┤               ├──────────────────────────────┤
│                          │               │                              │
│  GIL로 단일 스레드       │               │  네이티브 멀티스레드 가능    │
│  인터프리터 오버헤드     │   ──────►     │  컴파일 최적화              │
│                          │               │  제로 카피 문자열 처리       │
│                          │               │                              │
└──────────────────────────┘               └──────────────────────────────┘
```

AI 에이전트가 사용하는 도구는 **설치가 간단하고 빠르게 실행**되어야 합니다. pip 환경 설정 없이 바이너리 하나로 어디서든 실행되는 게 Rust의 최대 장점입니다.

## 원본 로직 보존 + 추가한 로직

### 원본에서 그대로 가져온 것

| 로직 | 원본 (Python) | Rust 포팅 |
| --- | --- | --- |
| BM25 인덱스 | `bm25s` 라이브러리 | `bm25.rs` — 직접 구현 |
| 시맨틱 임베딩 | `model2vec.StaticModel` | `encoder.rs` — safetensors 직접 로딩 |
| 벡터 검색 | `vicinity.CosineBasicBackend` | `encoder.rs` — ndarray dot product |
| RRF 하이브리드 융합 | `search.py` | `search.rs` — 동일한 1/(k+rank) 공식 |
| alpha 가중치 | `weighting.py` | `ranking/weighting.rs` — 심볼 0.3, 자연어 0.5 |
| 정의 부스팅 | `boosting.py` | `ranking/boosting.rs` — 동일 정규식 |
| 테스트 파일 페널티 | `penalties.py` | `ranking/penalties.rs` — 동일 패턴 |
| 토크나이저 | `tokens.py` | `tokens.rs` — camelCase/snake_case 분리 |
| 파일 워커 | `file_walker.py` | `file_walker.rs` — gitignore 존중 |

### 새로 추가한 것

**1. Tree-sitter AST 청킹** — 원본은 줄 기반으로 함수가 중간에 잘림. AST로 함수/클래스 단위 분할. 8개 언어 지원.

**2. 의존성 그래프 (deps/impact)** — 원본에 전혀 없던 기능. import/심볼 추출 → 파일 관계 그래프 → 전이적 영향 분석.

**3. grep 내장 매칭 라인** — 원본은 청크 범위만 반환. 청크 내 키워드 매칭 줄번호 + 코드 라인 추출.

**4. 형제 청크 부스팅** — 긴 파일이 여러 청크로 나뉠 때, 키워드 매칭 청크의 같은 파일 청크도 점수 올림.

**5. 그래프 부스팅** — 검색 상위 파일의 의존/역의존 파일도 점수 부여.

**6. 노이즈 필터** — 1위 점수의 12% 미만 결과 자동 제거. 원본은 항상 k개 전부 반환.

**7. TypeScript export 심볼 추출** — `export function/const/type/interface` 내부까지 파고들어 추출.

**8. Rust mod 선언 + @/ alias 해석** — `mod foo;` → `foo.rs` 매핑, `@/lib/templates` → 실제 파일 매핑.

## 원본과의 차이 요약

| 항목 | 원본 semble (Python) | semble_rs (Rust) |
| --- | --- | --- |
| 언어 | Python | Rust |
| 설치 | `pip install semble` | `cargo install --path .` |
| 검색 | search, find-related | search, find-related |
| **의존성 분석** | 없음 | **deps, impact** |
| 청킹 | 줄 기반 (고정 길이) | **Tree-sitter AST 기반** |
| 줄번호 | 청크 범위만 | **매칭 줄번호 + 코드 라인** |
| 노이즈 필터 | 없음 (항상 k개 반환) | **점수 기반 자동 필터** |
| 긴 파일 | 누락 발생 | **형제 청크 부스팅** |
| 그래프 부스팅 | 없음 | **의존 파일 검색 반영** |
| MCP 서버 | 있음 | 없음 (CLI만) |
| 모델 | potion-code-16M | potion-code-16M (동일) |
| 임베딩 라이브러리 | model2vec + vicinity | ndarray 직접 구현 |
| BM25 라이브러리 | bm25s | 직접 구현 |

## 실전 비교: semble_rs vs grep vs find+read

3개 프로젝트(TypeScript 56파일, TypeScript+Rust 351파일, Rust 43파일), 10개 시나리오에서 측정.

**semble_rs가 유리한 경우 — 대규모 프로젝트에서 컴포넌트 찾기:**

```
[terminal, 351파일] "GitPanel" 검색

find + read:  2개 파일 전체 읽기 → 220,853 토큰
grep -rn:     16줄 매칭 → 521 토큰
semble_rs:    2개 결과, 2/2 적중 → 734 토큰, 줄번호 포함  (vs read 100% 절감)
```

**grep이 유리한 경우 — 소규모 프로젝트에서 정확한 함수 찾기:**

```
[kekelink, 56파일] "deletePage" 검색

find + read:  2개 파일 전체 읽기 → 1,705 토큰
grep -rn:     3줄 매칭 → 98 토큰
semble_rs:    10개 결과, 2/2 적중 → 5,219 토큰  (grep보다 많음)
```

**합계:**

```
┌────────────────┬─────────────┬──────────────────────────────────────────────┐
│ 방법           │ 토큰 합계   │ 특징                                         │
├────────────────┼─────────────┼──────────────────────────────────────────────┤
│ find + read    │ 739,867     │ 파일 전체를 읽으므로 가장 많음               │
│ semble_rs      │  65,956     │ vs read 91% 절감, 적중률 100%               │
│ grep           │   4,176     │ 가장 적지만 의미 검색/의존성 분석 불가       │
└────────────────┴─────────────┴──────────────────────────────────────────────┘
```

**결론:**

- 파일 전체 읽기 대비 **91% 토큰 절감** — 에이전트가 Read를 반복하는 것보다 압도적
- 대규모 프로젝트(100+ 파일)에서 효과 극대화 (99%+ 절감)
- 소규모 프로젝트에서는 grep이 토큰 효율은 높지만, **deps/impact는 grep으로 불가능**
- 최적 전략: `semble_rs search`로 파일 좁히기 → 필요 시 `grep`으로 정밀 확인

## 설치

### 1. Rust 툴체인 설치

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. semble_rs 빌드 및 설치

```bash
git clone <repo>
cd semble
cargo install --path .
```

설치 후 `~/.cargo/bin/semble_rs`에 바이너리가 생성됩니다. 첫 실행 시 `potion-code-16M` 모델(\~60MB)이 HuggingFace에서 자동 다운로드됩니다.

### 3. 설치 확인

```bash
semble_rs --help
```

## 사용법

```bash
# 코드 검색 — 관련 파일 + 정확한 줄번호 반환
semble_rs search "deletePage" ./my-project
semble_rs search "handleImageUpload" ./my-project --json

# 의존성 확인 — 심볼, 의존 파일, 사용처
semble_rs deps src/lib/firestore.ts ./my-project

# 영향 분석 — 변경 시 깨지는 파일 전부
semble_rs impact src/lib/firestore.ts ./my-project

# 유사 코드 찾기
semble_rs find-related src/search.rs 91 .

# 토큰 절약 통계
semble_rs savings
```

## Claude Code 설치 및 연동

### 방법 1: CLAUDE.md (전역)

`~/.claude/CLAUDE.md`에 추가하면 모든 프로젝트에서 자동 적용:

```markdown
# semble_rs - Code Search & Dependency Analysis

`semble_rs`가 설치되어 있습니다. 코드 탐색 시 반드시 먼저 사용하세요.

## 규칙

1. 코드 찾기는 find/grep 전에 semble_rs search를 먼저 실행
2. 파일 수정 전에 semble_rs deps로 구조 파악
3. 리팩토링 전에 semble_rs impact로 영향 범위 확인

## 명령어

- 코드 찾기: `semble_rs search "query" . --json`
- 의존성: `semble_rs deps <file> . --json`
- 변경 영향: `semble_rs impact <file> . --json`
- 유사 코드: `semble_rs find-related <file> <line> . --json`
```

### 방법 2: 프로젝트별 CLAUDE.md

프로젝트 루트에 `CLAUDE.md`를 두면 해당 프로젝트에서만 적용:

```bash
cd ./my-project
cat > CLAUDE.md << 'EOF'
# 코드 탐색
semble_rs search "query" . --json 으로 코드 찾기
semble_rs deps <file> . --json 으로 의존성 확인
semble_rs impact <file> . --json 으로 변경 영향 확인
EOF
```

### 확인

새 Claude Code 세션을 열고 코드 탐색을 요청하면 semble_rs를 먼저 사용합니다:

```
❯ 이 프로젝트에서 인증 관련 코드 찾아줘

⏺ Bash(semble_rs search "authentication" . --json)
  ⎿ [{"chunk":{"file_path":"src/lib/firebase.ts", ...}]
```

## Codex 설치 및 연동

### AGENTS.md (전역)

`~/.codex/AGENTS.md`에 추가:

```markdown
# semble_rs - ALWAYS use before manual code exploration

`semble_rs` is installed at `~/.cargo/bin/semble_rs`.
You MUST use it before using find, grep, or reading files manually.

## Rules

1. NEVER start with find or manual file reading for code exploration
2. ALWAYS run semble_rs first to locate relevant code
3. Only use cat/read AFTER semble_rs narrows the target

## Commands

# Find code by keyword or symbol
~/.cargo/bin/semble_rs search "query" /path/to/project --json

# Check file dependencies and symbols
~/.cargo/bin/semble_rs deps <file> /path/to/project --json

# Check what breaks if a file changes
~/.cargo/bin/semble_rs impact <file> /path/to/project --json
```

### 프로젝트별 AGENTS.md

프로젝트 루트에 `AGENTS.md`를 두면 해당 프로젝트에서만 적용.

## 지원 언어

| 언어 | 검색 | Tree-sitter 청킹 | 의존성 그래프 |
| --- | --- | --- | --- |
| Rust | O | O | O |
| Python | O | O | O |
| JavaScript | O | O | O |
| TypeScript | O | O | O |
| Go | O | O | O |
| Java | O | O | O |
| C | O | O | O |
| C++ | O | O | O |
| Ruby, PHP, Swift, Kotlin 등 | O | 줄 기반 fallback | \- |

## 라이선스

MIT