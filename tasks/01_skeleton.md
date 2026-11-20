# 작업 1: 프로젝트 골격

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 목표

빈 저장소에 Cargo 프로젝트 골격을 만든다. 로직은 구현하지 않는다. 이후 작업이 채울 모듈 파일을 문서 주석만 있는 상태로 만든다.

## 대상 파일(모두 신규)

- `Cargo.toml`
- `.gitignore`
- `src/main.rs`, `src/lib.rs`
- `src/paths.rs`, `src/validate.rs`, `src/keys.rs`, `src/tui.rs`
- `src/config/mod.rs`, `src/config/lexer.rs`, `src/config/model.rs`, `src/config/reader.rs`, `src/config/edit.rs`, `src/config/diff.rs`, `src/config/writer.rs`
- `src/ssh/mod.rs`, `src/ssh/args.rs`, `src/ssh/effective.rs`, `src/ssh/exec.rs`
- `src/dialog/mod.rs`
- `src/app/mod.rs`
- `src/ui/mod.rs`

## 내용

### Cargo.toml

아래 내용 그대로 작성한다. 버전은 바꾸지 않는다.

```toml
[package]
name = "sshtui"
version = "0.1.0"
edition = "2024"
description = "A TUI wrapper around the system ssh client that stores hosts in ~/.ssh/config"
publish = false

[dependencies]
anyhow = "1.0.104"
crossterm = "0.29.0"
glob = "0.3.4"
ratatui = "0.30.2"
similar = "3.2.1"
tempfile = "3.27.0"
thiserror = "2.0.21"
unicode-width = "0.2.3"
which = "8.0.6"

[target.'cfg(windows)'.dependencies]
rfd = { version = "0.18.0", default-features = false }
```

### .gitignore

```
/target
/logs
```

### src/lib.rs

- 크레이트 문서 주석(`//!`) 2-3줄.
- `pub mod app; pub mod config; pub mod dialog; pub mod keys; pub mod paths; pub mod ssh; pub mod tui; pub mod ui; pub mod validate;`

### src/main.rs

- `fn main() -> std::process::ExitCode`.
- 인자 처리: `--version` 또는 `-V`이면 `sshtui <CARGO_PKG_VERSION>` 출력 후 성공 종료. `--help` 또는 `-h`이면 사용법 몇 줄 출력 후 성공 종료. 그 외 알 수 없는 인자는 표준 오류에 메시지 출력 후 종료 코드 2.
- 인자가 없으면 표준 오류에 `sshtui: the TUI is not implemented yet` 출력 후 종료 코드 1. (작업 10에서 교체 예정)
- 인자 해석은 `fn parse_args(args: &[String]) -> CliAction` 순수 함수로 분리하고 `enum CliAction { Run, Version, Help, Invalid(String) }`을 `main.rs` 안에 둔다. 이 함수의 단위 테스트 4개(인자 없음, `--version`, `-h`, 알 수 없는 인자)를 작성한다.

### 나머지 모듈 파일

- 각 파일에 그 모듈의 역할을 설명하는 `//!` 문서 주석만 둔다. 타입과 함수는 만들지 않는다.
- `config/mod.rs`는 `pub mod diff; pub mod edit; pub mod lexer; pub mod model; pub mod reader; pub mod writer;`
- `ssh/mod.rs`는 `pub mod args; pub mod effective; pub mod exec;`
- 모듈 역할:
  - `paths`: ssh 디렉터리 위치 결정(환경변수 `SSHTUI_SSH_DIR` 주입)과 `~` 확장
  - `validate`: 닉네임, 주소, 유저, 포트 검증
  - `keys`: ssh 디렉터리 직속의 비밀키 탐색
  - `tui`: 터미널 초기화와 복원, 이벤트 루프
  - `config::lexer`: ssh_config 줄 단위 토큰화
  - `config::model`: 원문 바이트를 보존하는 파일, 블록, 호스트 항목 타입
  - `config::reader`: `Include` 확장과 `Host` 항목 수집
  - `config::edit`: 호스트 추가, 수정, 이름 변경, 삭제의 순수 함수
  - `config::diff`: 저장 전 확인용 줄 diff
  - `config::writer`: 외부 변경 감지, 백업, 임시 파일 교체
  - `ssh::args`: ssh 인자 배열과 미리보기 문자열 생성
  - `ssh::effective`: `ssh -G` 실행과 출력 파싱
  - `ssh::exec`: ssh 실행 파일 탐색과 실행
  - `dialog`: 네이티브 파일 선택 다이얼로그 trait과 OS별 구현
  - `app`: 화면 상태와 상태 전이
  - `ui`: ratatui 렌더링

## 이 작업에 한정된 검증 방법

의존성이 아직 다운로드되지 않았고 샌드박스에 네트워크가 없다. 그래서 이 작업에서는 `cargo build`, `cargo clippy`, `cargo test`, `cargo fmt`를 실행하지 않는다. 대신 아래만 실행한다.

```
rustfmt --check --edition 2024 src/main.rs src/lib.rs
```

`cargo` 명령이 의존성 문제로 실패하더라도 `Cargo.toml`을 바꾸거나 우회하지 않는다. 검수자가 의존성을 내려받은 뒤 `AGENTS.md`의 완료 조건 3개 명령을 실행한다.

## 완료 조건

- 위 대상 파일이 모두 존재하고 지시한 내용과 일치
- `rustfmt --check` 통과
- 검수자 환경에서 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 통과(테스트 4개)
