# 작업 25: 비밀번호 인증 옵션과 공개 키 설치 로직

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 목표

화면과 무관한 부분만 만든다. 이 작업에서는 폼 동작과 화면을 바꾸지 않는다(작업 27에서 연결).

1. config 편집 모델에 `PasswordAuthentication` 옵션 추가
2. 1회 접속 대상에 "비밀번호 입력 끄기" 플래그 추가
3. 서버의 `authorized_keys`에 공개 키를 넣는 ssh 호출의 인자 구성(`ssh-copy-id`와 같은 일을 시스템 `ssh`로 수행. Windows에는 `ssh-copy-id`가 없으므로 직접 구성)
4. 표준 입력으로 데이터를 넘기며 ssh를 실행하는 함수

## 대상 파일

- `src/config/edit.rs`, `src/config/mod.rs`(재공개가 필요할 때)
- `src/ssh/args.rs`, `src/ssh/keysetup.rs`(신규), `src/ssh/mod.rs`, `src/ssh/exec.rs`
- 컴파일 유지를 위한 기계적 수정만 허용: `src/app/form.rs`, `src/ssh/effective.rs`, 그리고 테스트 코드(`src/app/tests.rs`, `src/ui/tests.rs`, `src/config/plan.rs`, `src/system.rs` 등)의 구조체 리터럴과 `match`에 새 필드와 새 갈래를 채우는 것. 이 파일들에서 동작 변경과 단언 변경 금지.

`Cargo.toml` 수정 금지.

## 1. `PasswordAuthentication` 옵션 (`src/config/edit.rs`)

- `OptionKey::PasswordAuthentication` 추가. `keyword()`는 `PasswordAuthentication`. `OptionKey::ALL`은 5개가 되고 순서는 `HostName`, `User`, `Port`, `IdentityFile`, `PasswordAuthentication`(새 줄을 쓰는 순서).
- `BlockFields`, `OptionChanges`, `NewHost`에 `password_authentication` 필드 추가(각각 `FieldState`, `Change`, `Option<String>`). `get()`류 접근자도 갱신.
- `block_fields`, `add_host`, `apply_host_edit`가 기존 4개 옵션과 같은 규칙으로 이 옵션을 다룬다(키워드 대소문자 무시, `key=value` 형식, 따옴표, 최소 변경, 중복이나 인자 여러 개는 `Unrepresentable`).
- 값 검증은 하지 않는다(폼이 `no`만 쓴다).

테스트 추가: `PasswordAuthentication no`를 읽어 `Value("no")`, `passwordauthentication=no` 읽기, `add_host`로 새 블록에 마지막 줄로 쓰기, `Set`으로 기존 블록에 추가(다른 줄 바이트 보존), 기존 값 변경, `Remove`, 중복 줄은 `Unrepresentable`.

`src/app/form.rs`는 컴파일되도록만 고친다: `NewHost`에는 `password_authentication: None`, `OptionChanges`에는 `Change::Keep`, 플레이스홀더 `match`의 새 갈래는 `None`.

## 2. 1회 접속의 비밀번호 입력 끄기 (`src/ssh/args.rs`)

`ConnectTarget::Direct`에 필드 추가:

```rust
/// Adds `-o PasswordAuthentication=no` so ssh never asks for a password.
disable_password: bool,
```

`connect_args`는 이 값이 참이면 접두 인자 바로 뒤에 `-o`, `PasswordAuthentication=no` 두 인자를 넣는다(그 뒤에 `-p`, `-i`, 대상 순서는 기존과 같음). 기존 호출부와 테스트의 리터럴에는 `disable_password: false`를 채운다. 테스트 추가 1개.

## 3. 공개 키 설치 (`src/ssh/keysetup.rs`)

```rust
/// Remote command that appends the public key read from stdin to authorized_keys.
pub const REMOTE_INSTALL_SCRIPT: &str = /* 아래 문자열 그대로 */;

/// Builds the ssh arguments that install a public key for the given target.
pub fn install_args(
    paths: &SshPaths,
    prefix: &[OsString],
    target: &ConnectTarget,
) -> Result<Vec<OsString>, ArgsError>;

/// Returns the public key path for a private key path typed in the form.
pub fn public_key_path(paths: &SshPaths, key_file: &str) -> PathBuf;

/// Why a public key cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum KeySetupError { /* Missing { path }, Read { path, source }, NotAPublicKey { path } */ }

/// Reads and validates the single public key line of a `.pub` file.
pub fn read_public_key(path: &Path) -> Result<String, KeySetupError>;
```

`REMOTE_INSTALL_SCRIPT`(한 줄, 아래 그대로. 안쪽에는 작은따옴표를 쓰지 않는다):

```
exec sh -c 'umask 077; mkdir -p "$HOME/.ssh" || exit 1; cd "$HOME/.ssh" || exit 1; key=$(cat); [ -n "$key" ] || { echo "sshtui: no key received" >&2; exit 1; }; touch authorized_keys || exit 1; if grep -qxF -- "$key" authorized_keys; then echo "sshtui: key already installed"; else [ -z "$(tail -c 1 authorized_keys)" ] || echo >> authorized_keys; printf "%s\n" "$key" >> authorized_keys || exit 1; echo "sshtui: key installed"; fi; if command -v restorecon >/dev/null 2>&1; then restorecon -F "$HOME/.ssh" "$HOME/.ssh/authorized_keys" >/dev/null 2>&1 || true; fi'
```

`install_args`:

- `target`의 `disable_password`를 거짓으로 둔 것으로 `connect_args`를 호출해 검증과 기본 인자를 얻는다.
- 접두 인자 바로 뒤에 `-o`, `PasswordAuthentication=yes`를 넣는다(명령줄 옵션이 config의 `PasswordAuthentication no`보다 우선하므로, 이미 비밀번호 입력을 끈 호스트에서도 설치할 수 있음).
- 맨 끝에 `REMOTE_INSTALL_SCRIPT`를 인자 하나로 붙인다. 셸을 거치지 않는다.
- 예: 별칭 `web`, 접두 인자 없음 → `["-o", "PasswordAuthentication=yes", "web", <스크립트>]`

`public_key_path`: 입력의 앞뒤 공백을 떼고 `paths.expand_tilde`로 확장. 이미 `.pub`로 끝나면 그 경로, 아니면 파일 이름 끝에 `.pub`를 붙인 경로(`with_extension`을 쓰지 않는다. `id_ed25519` → `id_ed25519.pub`, `my.key` → `my.key.pub`).

`read_public_key`:

- 없는 파일은 `Missing`, 그 밖의 읽기 오류는 `Read`.
- 처음 16KiB까지만 읽어 첫 줄을 사용하고 앞뒤 공백 제거.
- `crate::keys::parse_public_key_line`이 `None`이거나 줄에 `PRIVATE KEY`가 들어 있으면 `NotAPublicKey`. 비밀키 파일을 서버로 보내는 일이 없어야 한다.
- 오류 메시지에는 경로만 넣고 파일 내용을 넣지 않는다.

테스트:

- `install_args`: 별칭, 1회 접속(포트와 키 파일 포함, `disable_password: true`여도 `PasswordAuthentication=no`가 없고 `=yes`가 있음), 주입된 config 접두 인자(`-F <경로>`) 뒤에 옵션이 오는지, 잘못된 대상의 오류 전달, 마지막 인자가 스크립트와 동일
- `public_key_path`: `~/.ssh/id_ed25519`, 이미 `.pub`, 점이 든 이름, 앞뒤 공백
- `read_public_key`(`tempfile`): 정상(코멘트 포함), 뒤에 줄이 더 있는 파일, 없는 파일, 비밀키 헤더 파일, 빈 파일
- `#[cfg(unix)]` 스크립트 실행 테스트: `std::process::Command`로 `sh -c <REMOTE_INSTALL_SCRIPT>`를 `HOME=<임시 디렉터리>` 환경과 표준 입력의 키 한 줄로 실행. (a) 첫 실행 후 `authorized_keys`에 키 한 줄, 출력에 `key installed` (b) 다시 실행하면 줄 수 그대로, 출력에 `already installed` (c) 줄 끝 문자 없이 끝나는 기존 `authorized_keys`에 추가해도 기존 줄과 붙지 않음 (d) 빈 입력은 종료 코드 1이고 파일 내용 불변. 이 테스트는 `HOME`을 임시 디렉터리로 바꾼 자식 프로세스만 실행하고 실제 홈을 건드리지 않는다.

## 4. 표준 입력을 넘기는 실행 (`src/ssh/exec.rs`)

```rust
/// Runs ssh as a child with `input` on stdin while stdout and stderr stay attached to the terminal.
/// Returns the child's exit code.
pub fn run_with_input(ssh: &Path, args: &[OsString], input: &[u8]) -> Result<i32, SshError>;
```

- stdin은 파이프, stdout과 stderr는 상속. `input`을 쓰고 stdin을 닫은 뒤 종료를 기다린다. 자식이 먼저 끝나 쓰기가 실패해도(깨진 파이프) 종료 코드는 반환한다.
- 종료 코드가 없는 경우(신호로 종료)의 값은 기존 자식 실행 경로와 같은 규칙.
- `#[cfg(unix)]` 테스트: 임시 디렉터리에 실행 권한을 준 셸 스크립트(표준 입력을 파일로 저장하고 인자를 파일에 기록한 뒤 `exit 7`)를 만들어 인자, 입력, 종료 코드 확인. 없는 실행 파일은 `SshError::Io`.

## 제약

- 실제 `~/.ssh`와 실제 홈 접근 금지. 키 파일 내용을 출력하거나 오류 메시지에 넣지 않는다.
- 라이브러리 코드에서 `unwrap`, `expect`, `panic!` 금지. 공개 항목에 영어 문서 주석.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- 기존 테스트의 단언 변경 없음(허용된 리터럴 필드 추가 제외). 고친 리터럴의 위치를 최종 보고에 요약
- `git status`에 대상 파일과 허용된 파일만 변경으로 표시
