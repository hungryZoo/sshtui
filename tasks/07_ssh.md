# 작업 7: ssh 인자 생성, `ssh -G` 해석, 실행

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `src/paths.rs`, `src/validate.rs`, `src/config/edit.rs`의 `OptionKey`를 사용한다.

## 목표

1. `ssh::args`: 접속 인자 배열과 미리보기 문자열(순수 함수)
2. `ssh::effective`: `ssh -G <alias>` 출력 파싱과 입력값 대조(순수 함수), 실행을 감싼 trait
3. `ssh::exec`: ssh 실행 파일 탐색과 실행

ssh는 항상 인자 배열로 실행한다. 셸을 거치는 코드(`sh -c`, `cmd /C`)는 금지한다.

## 대상 파일

- `src/ssh/args.rs`, `src/ssh/effective.rs`, `src/ssh/exec.rs`, `src/ssh/mod.rs`(재노출만)

## src/ssh/args.rs

```rust
/// What a connection targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectTarget {
    /// A saved alias: `ssh <alias>`.
    Alias(String),
    /// A one-off connection built from form fields. Empty strings mean "not given".
    Direct { user: String, address: String, port: String, key_file: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error(transparent)]
    Validation(#[from] crate::validate::ValidationError),
    #[error("host alias must not be empty or start with '-'")]
    InvalidAlias,
}

/// `-F` arguments that keep ssh inside an injected ssh directory.
/// Not overridden: empty. Overridden and `config_exists`: `["-F", <paths.config_file()>]`.
/// Overridden and no config file: `["-F", "none"]`.
pub fn config_file_args(paths: &SshPaths, config_exists: bool) -> Vec<OsString>;

/// Arguments after the program name. `prefix` is the result of `config_file_args`.
pub fn connect_args(
    paths: &SshPaths,
    prefix: &[OsString],
    target: &ConnectTarget,
) -> Result<Vec<OsString>, ArgsError>;

/// Human-readable command line for display: `ssh` followed by the arguments, each quoted
/// when needed. For display only; it is never executed.
pub fn preview(args: &[OsString]) -> String;
```

`connect_args` 규칙:

- `Alias(a)`: 비어 있거나 `-`로 시작하면 `InvalidAlias`. 결과는 `prefix + [a]`. (기존 config의 별칭은 sshtui의 닉네임 규칙보다 넓을 수 있으므로 `validate_nickname`은 적용하지 않는다.)
- `Direct`: `validate_address`, `validate_user`, `parse_port`, `validate_key_file`을 모두 통과해야 한다. 결과는 `prefix` 다음에 아래 순서다. 비어 있는 필드의 옵션은 넣지 않는다.
  1. 포트가 있으면 `-p`, `<포트 숫자>` (선행 0 제거된 10진수)
  2. 키 파일이 있으면 `-i`, `<paths.expand_tilde(key_file)>`
  3. 목적지: 유저가 있으면 `<user>@<address>`, 없으면 `<address>`
- 옵션과 값은 항상 별도 인자다(`-p22` 형태 금지).

`preview` 규칙: 인자가 비어 있거나 `A-Z a-z 0-9 @ % + = : , . / - _ ~` 이외의 문자를 포함하면 작은따옴표로 감싼다. 인자 안의 작은따옴표는 `'\''`로 쓴다. UTF-8이 아닌 인자는 lossy 변환한다.

테스트:

1. `Alias("web")` → `["web"]`, 미리보기 `ssh web`
2. `Alias("")`, `Alias("-oProxyCommand=x")` → `InvalidAlias`
3. `Direct` 주소만 → `["example.com"]`
4. 모든 필드 → `["-p", "2222", "-i", "<home>/.ssh/id_ed25519", "alice@example.com"]` (key_file 입력은 `~/.ssh/id_ed25519`)
5. 포트 `022` → `"22"`
6. 검증 실패 전파: 주소 `-x`, 유저 `-l`, 포트 `0`, 키 파일 `a"b`, 빈 주소
7. `config_file_args` 3가지 경우와, prefix가 결과 맨 앞에 오는지
8. 공백이 든 키 경로가 인자 하나로 유지되고 미리보기에서 따옴표 처리(`ssh -i '/keys/my key' host`)
9. `preview`의 작은따옴표 이스케이프, 빈 인자(`''`)
10. IPv6 주소 `::1`과 유저 → `alice@::1`

## src/ssh/effective.rs

`ssh -G <alias>` 출력은 줄마다 `<소문자 키> <값>` 형태다. 값은 줄의 첫 공백 뒤 전체이며 공백을 포함할 수 있고 따옴표가 없다. `identityfile`은 여러 줄 나올 수 있다. `hostname`은 소문자로 출력된다. 예:

```
host Web
user alice
hostname web.example.com
port 2222
identityfile ~/.ssh/id_test
identityfile /tmp/my dir/key
identityfile /abs/other
```

```rust
/// Values ssh would actually use for an alias.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectiveConfig {
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<String>,
    /// In the order printed by ssh.
    pub identity_files: Vec<String>,
}

/// Pure parser for `ssh -G` output. Unknown keys and malformed lines are ignored.
pub fn parse_ssh_g(output: &str) -> EffectiveConfig;

/// Arguments for the query: `prefix + ["-G", alias]`.
pub fn ssh_g_args(prefix: &[OsString], alias: &str) -> Vec<OsString>;

/// Values the user entered for a saved host. Empty strings mean "not given" and are not checked.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExpectedConfig {
    pub host_name: String,
    pub user: String,
    pub port: String,
    pub identity_file: String,
}

/// One entered value that ssh would not use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub key: OptionKey,
    pub expected: String,
    /// What ssh reports instead (for IdentityFile: all reported files joined with ", "; "-" when none).
    pub actual: String,
}
impl std::fmt::Display for Mismatch { /* e.g. "User: saved \"alice\" but ssh will use \"root\"" */ }

/// Pure comparison of entered values against the effective configuration.
pub fn find_mismatches(
    paths: &SshPaths,
    expected: &ExpectedConfig,
    effective: &EffectiveConfig,
) -> Vec<Mismatch>;

/// Source of effective configurations; lets the UI be tested without running ssh.
pub trait EffectiveResolver {
    fn resolve(&self, alias: &str) -> Result<EffectiveConfig, SshError>;
}

/// Runs the system ssh.
#[derive(Debug, Clone)]
pub struct SystemResolver { /* ssh: PathBuf, prefix: Vec<OsString> */ }
impl SystemResolver { pub fn new(ssh: PathBuf, prefix: Vec<OsString>) -> Self; }
impl EffectiveResolver for SystemResolver { /* exec::run_capture(ssh, ssh_g_args(prefix, alias)) then parse */ }
```

`find_mismatches` 규칙(비어 있는 입력 필드는 검사하지 않음, 결과 순서는 HostName, User, Port, IdentityFile):

- HostName: ASCII 대소문자 무시 비교.
- User: 완전 일치.
- Port: 양쪽을 `u16`으로 해석해 비교. 해석 실패 시 문자열 비교.
- IdentityFile: 입력값과 `identity_files`의 각 항목을 모두 `paths.expand_tilde`로 확장한 뒤 비교하여 일치하는 항목이 하나도 없으면 불일치.
- `effective`에 해당 값이 없으면(`None`) 불일치이며 `actual`은 `-`.

`SystemResolver::resolve`: 별칭이 비어 있거나 `-`로 시작하면 실행하지 않고 `SshError::InvalidAlias`.

테스트:

11. 위 예시 출력 파싱(identityfile 3개, 공백 포함 경로 유지)
12. 빈 출력, 값 없는 줄(`user`), 모르는 키, CRLF 줄바꿈
13. `find_mismatches`: 전부 일치(HostName 대소문자 차이 포함) → 빈 벡터
14. User가 다름(`alice` 입력, `root` 적용) → 1건, `key`와 `expected`와 `actual` 확인
15. Port `022` 입력과 `22` 적용은 일치, `2222`와 `22`는 불일치
16. IdentityFile: `~/.ssh/id_test` 입력과 `<home>/.ssh/id_test` 적용은 일치, 목록에 없으면 불일치이며 `actual`이 `, `로 이어진 목록
17. 빈 입력 필드는 적용값이 무엇이든 검사하지 않음
18. `Mismatch`의 `Display` 문자열
19. `ssh_g_args`
20. 가짜 `EffectiveResolver` 구현을 테스트 모듈에 만들어 trait 객체(`&dyn EffectiveResolver`)로 사용 가능함을 확인
21. 실제 ssh 통합 테스트 1건: `exec::find_ssh()`가 실패하면 조용히 반환(테스트 통과). 성공하면 임시 디렉터리에 아래 config를 쓰고 `SystemResolver::new(ssh, ["-F", <임시 config>])`로 `resolve("web")`을 호출해 `host_name == "web.example.com"`, `user == "alice"`, `port == "2222"`, `identity_files`에 `/abs/key` 포함을 확인한다. 이 테스트의 config에는 `Include`를 넣지 않는다(ssh가 상대 Include를 실제 `~/.ssh` 기준으로 풀기 때문). `-F` 없이 ssh를 실행하는 테스트는 금지한다.

```
Host web
    HostName Web.Example.COM
    User alice
    Port 2222
    IdentityFile /abs/key
```

## src/ssh/exec.rs

```rust
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("the `ssh` command was not found in PATH; install the OpenSSH client and make sure `ssh` is on PATH")]
    NotFound,
    #[error("host alias must not be empty or start with '-'")]
    InvalidAlias,
    #[error("cannot run {program}: {source}")]
    Io { program: PathBuf, #[source] source: std::io::Error },
    #[error("ssh exited with {status}: {stderr}")]
    Failed { status: String, stderr: String },
}

/// Locates `ssh` using the PATH of the current process.
pub fn find_ssh() -> Result<PathBuf, SshError>;

/// Locates `ssh` in an explicit search path (testable). `cwd` resolves relative PATH entries.
pub fn find_ssh_in(search_path: Option<&OsStr>, cwd: &Path) -> Result<PathBuf, SshError>;

/// Runs ssh with stdin closed and returns its standard output (lossy UTF-8).
/// A non-zero exit status is `SshError::Failed` with trimmed stderr.
pub fn run_capture(ssh: &Path, args: &[OsString]) -> Result<String, SshError>;

/// Hands the terminal over to ssh.
/// Unix: replaces the current process (`CommandExt::exec`); returns only on failure.
/// Windows and others: runs ssh as a child with inherited stdio, waits, and returns its exit code
/// (1 when the process was terminated without a code).
pub fn run_interactive(ssh: &Path, args: &[OsString]) -> Result<i32, SshError>;
```

- `find_ssh_in`은 `which::which_in("ssh", search_path, cwd)`를 사용한다. `which` 8.0.6의 API는 `~/.cargo/registry/src/*/which-8.0.6/`에서 확인한다.
- `run_capture`는 `stdin(Stdio::null())`, stdout과 stderr는 파이프로 받는다.
- `run_interactive`는 호출 전에 터미널이 복원되어 있다고 가정한다(복원은 작업 10의 책임).

테스트:

22. `find_ssh_in`: 임시 디렉터리에 가짜 실행 파일(`#[cfg(unix)]`: `ssh`를 만들고 0755, `#[cfg(windows)]`: `ssh.exe`)을 두고 그 디렉터리를 search path로 주면 그 경로를 찾음
23. `find_ssh_in`: 빈 임시 디렉터리만 search path로 주면 `NotFound`, 오류 메시지에 `PATH` 포함. `search_path`가 `None`이어도 `NotFound`
24. (`#[cfg(unix)]`) `run_capture`: 임시 디렉터리에 셸 스크립트(`#!/bin/sh`)로 만든 가짜 프로그램으로 stdout 수집, 0이 아닌 종료 코드에서 `Failed`와 stderr 내용, 인자가 공백을 포함해도 하나의 인자로 전달됨(스크립트가 `$#`와 `$1`을 출력)
25. `run_capture`: 없는 프로그램 경로는 `Io`

`run_interactive`는 프로세스를 교체하므로 단위 테스트를 작성하지 않는다.

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- 위 테스트가 모두 존재하고 기존 테스트 유지
