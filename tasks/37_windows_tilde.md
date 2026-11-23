# 작업 37: Windows에서 `~` 확장 경로의 구분자 혼용 수정

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 배경

GitHub Actions의 `windows-latest`에서 테스트가 처음 실행되었다. 530개 가운데 527개가 통과하고 3개가 같은 원인으로 실패했다. 다른 OS에서는 모두 통과한다.

## 문제 위치

```
test ssh::args::tests::builds_direct_arguments_in_order_and_expands_tilde ... FAILED
  left: ["-p", "2222", "-i", "C:\\Users\\RUNNER~1\\AppData\\Local\\Temp\\.tmpmq5dkQ\\.ssh/id_ed25519", "alice@example.com"]
 right: ["-p", "2222", "-i", "C:\\Users\\RUNNER~1\\AppData\\Local\\Temp\\.tmpmq5dkQ\\.ssh\\id_ed25519", "alice@example.com"]

test app::tests::unnamed_form_connects_directly_without_writing ... FAILED        (src\app\tests.rs:1128)
  left: ... "-i", "C:\\...\\home\\.ssh/id", ...
 right: ... "-i", "C:\\...\\home\\.ssh\\id", ...

test app::tests::direct_key_install_ignores_input_and_connects_only_after_success ... FAILED   (src\app\tests.rs:2893)
  left: ... "-i", "C:\\...\\home\\.ssh/id_a", ...
 right: ... "-i", "C:\\...\\home\\.ssh\\id_a", ...
```

`SshPaths::expand_tilde("~/.ssh/id")`가 홈 경로에 `.ssh/id` 문자열을 그대로 붙여, Windows에서 `\`와 `/`가 섞인 경로를 만든다. 테스트는 `home.join(".ssh").join("id")` 형태(플랫폼 구분자)를 기대한다. 테스트가 옳고 구현을 고친다.

## 대상 파일

- `src/paths.rs`
- 같은 원인으로 구분자가 섞이는 곳이 더 있으면 그 파일(예: `src/ssh/keysetup.rs`의 `public_key_path`). 찾아서 함께 고치고 최종 보고에 적는다.

## 기대 동작

- `expand_tilde`의 결과는 플랫폼의 구분자만 쓴다. `~/a/b`는 Unix에서 `<home>/a/b`, Windows에서 `<home>\a\b`. Windows에서는 입력의 `/`와 `\`를 모두 구분자로 취급한다(`~\a\b`와 `~/a\b` 포함).
- Unix에서의 결과는 지금과 바이트 단위로 같다. 특히 `\`는 Unix에서 구분자가 아니므로 그대로 둔다.
- `~`만 있는 입력, `~`로 시작하지 않는 입력, 홈이 없는 경우의 동작은 그대로.
- `..`이나 `.` 구성 요소를 해석하거나 제거하지 않는다(경로 정규화는 구분자에 한정).

## 테스트

- `src/paths.rs`에 추가: `~/a/b`의 결과가 `home.join("a").join("b")`와 같음(모든 OS에서 성립하도록 작성). `#[cfg(windows)]` 테스트로 `~\a\b`, `~/a\b`, `~/a/../b`(`..` 유지) 확인. `#[cfg(unix)]` 테스트로 `~/a\b`의 `\`가 유지됨 확인.
- 위 3개 실패 테스트는 수정하지 않는다.

로컬은 macOS라 Windows 테스트를 실행할 수 없다. `cargo check --offline --all-targets --target x86_64-pc-windows-msvc`와 `cargo clippy --offline --all-targets --target x86_64-pc-windows-msvc -- -D warnings`가 통과하는지 확인하고, Windows에서의 실행 결과는 "미검증(CI에서 확인 예정)"으로 보고한다.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- `git status`에 대상 파일만 변경으로 표시
