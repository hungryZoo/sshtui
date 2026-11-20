# 작업 2: paths와 validate

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 목표

ssh 디렉터리 경로 주입, `~` 확장, 입력값 검증을 순수 함수로 구현하고 단위 테스트를 작성한다.

## 대상 파일

- `src/paths.rs`
- `src/validate.rs`

## src/paths.rs

```rust
/// Environment variable that overrides the ssh directory (default: `~/.ssh`).
pub const SSH_DIR_ENV: &str = "SSHTUI_SSH_DIR";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathsError {
    /// Neither an override nor a home directory is available.
    #[error("cannot determine the ssh directory: no home directory found; set SSHTUI_SSH_DIR")]
    NoHome,
    /// The override could not be made absolute.
    #[error("invalid SSHTUI_SSH_DIR value {0:?}: {1}")]
    InvalidOverride(std::ffi::OsString, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshPaths { /* private fields: home: Option<PathBuf>, ssh_dir: PathBuf, overridden: bool */ }

impl SshPaths {
    /// Pure resolution. `override_dir` is the raw value of `SSHTUI_SSH_DIR` (None when unset),
    /// `home` is the user's home directory, `cwd` is used to absolutize a relative override.
    pub fn resolve(
        override_dir: Option<&std::ffi::OsStr>,
        home: Option<&Path>,
        cwd: &Path,
    ) -> Result<Self, PathsError>;

    /// Reads `SSHTUI_SSH_DIR`, `std::env::home_dir()` and the current directory, then calls `resolve`.
    pub fn from_env() -> Result<Self, PathsError>;

    pub fn ssh_dir(&self) -> &Path;
    /// `<ssh_dir>/config`
    pub fn config_file(&self) -> PathBuf;
    /// True when the directory came from `SSHTUI_SSH_DIR`.
    pub fn is_overridden(&self) -> bool;
    pub fn home(&self) -> Option<&Path>;

    /// Expands a leading `~` (exactly `~`, or `~/...`; on Windows also `~\...`) to the home directory.
    /// Any other input, including `~user/...`, is returned unchanged. Without a home directory the
    /// input is returned unchanged.
    pub fn expand_tilde(&self, input: &str) -> PathBuf;

    /// Inverse of `expand_tilde` for display and storage: when `path` is the home directory or lies
    /// under it, returns `~` or `~/rest` with `/` separators; otherwise returns the path as a string
    /// (lossy conversion). Comparison is component-wise, not a string prefix test
    /// (`/home/al` must not abbreviate `/home/alice/x`).
    pub fn abbreviate_home(&self, path: &Path) -> String;
}
```

규칙:

- `override_dir`가 `None`이거나 빈 문자열이면 미지정으로 본다. 이때 `ssh_dir = <home>/.ssh`, `overridden = false`. `home`도 없으면 `PathsError::NoHome`.
- `override_dir`가 있으면 `ssh_dir`는 그 경로이고 `overridden = true`. 상대 경로면 `cwd.join(...)`으로 절대 경로를 만든다. 파일 시스템에는 접근하지 않는다(존재 확인, canonicalize 금지).
- `override_dir`가 있으면 `home`이 없어도 성공한다.
- `home`은 `~` 확장에만 쓴다. override 여부와 무관하게 실제 홈을 가리킨다.
- `std::env::home_dir()`가 현재 툴체인에서 deprecated 경고를 내면 `allow`로 끄지 말고 최종 보고에 적는다.

테스트(임시 디렉터리 불필요, 가짜 경로 문자열 사용):

1. override 미지정 시 `<home>/.ssh`와 `config_file()`이 `<home>/.ssh/config`
2. 빈 override는 미지정과 동일
3. 절대 경로 override 적용, `is_overridden()`이 true
4. 상대 경로 override가 `cwd` 기준 절대 경로로 변환
5. home과 override 모두 없으면 `NoHome`
6. home이 없어도 override가 있으면 성공
7. `expand_tilde`: `~`, `~/a/b`, `~user/x`(불변), `/abs`(불변), `rel/~/x`(불변), 빈 문자열(불변)
8. home이 없을 때 `expand_tilde("~/a")`는 불변
9. `abbreviate_home`: home 자체, home 하위 경로, home 밖 경로, 접두사만 같은 형제 경로(`/home/al` 대 `/home/alice`)
10. `expand_tilde(abbreviate_home(p)) == p` (home 하위 경로)

## src/validate.rs

```rust
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("{field} is required")]
    Empty { field: Field },
    #[error("{field} must not start with '-'")]
    LeadingDash { field: Field },
    #[error("{field} must not contain {ch:?}")]
    ForbiddenChar { field: Field, ch: char },
    #[error("{field} must not contain whitespace")]
    Whitespace { field: Field },
    #[error("{field} must not contain control characters")]
    ControlChar { field: Field },
    #[error("port must be a number between 1 and 65535")]
    InvalidPort,
    #[error("nickname {0:?} is already in use")]
    DuplicateNickname(String),
}

/// Form field a validation error refers to. `Display` prints a lowercase label
/// ("nickname", "user", "address", "port", "key file").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field { Nickname, User, Address, Port, KeyFile }

pub fn validate_nickname(nickname: &str) -> Result<(), ValidationError>;
pub fn check_nickname_unique<'a, I>(nickname: &str, existing: I) -> Result<(), ValidationError>
where I: IntoIterator<Item = &'a str>;
pub fn validate_address(address: &str) -> Result<(), ValidationError>;
pub fn validate_user(user: &str) -> Result<(), ValidationError>;
pub fn parse_port(port: &str) -> Result<Option<u16>, ValidationError>;
pub fn validate_key_file(path: &str) -> Result<(), ValidationError>;
```

검사 순서는 아래 나열 순서를 따른다(먼저 걸린 규칙의 오류 반환). 모든 함수는 입력을 trim하지 않는다. 앞뒤 공백도 공백 포함으로 거부한다(`validate_key_file` 제외, 아래 참고).

- `validate_nickname`
  1. 빈 문자열: `Empty`
  2. 제어 문자(`char::is_control`) 포함: `ControlChar`
  3. 공백 문자(`char::is_whitespace`) 포함: `Whitespace`
  4. `-`로 시작: `LeadingDash`
  5. `=`로 시작: `ForbiddenChar { ch: '=' }` (ssh_config에서 키워드 뒤의 `=` 하나는 구분자로 소비됨)
  6. `*`, `?`, `!`, `#`, `"` 포함: `ForbiddenChar` (처음 나온 문자)
- `check_nickname_unique`: `existing`에 대소문자까지 같은 문자열이 있으면 `DuplicateNickname`. 형식 검증은 하지 않는다.
- `validate_address` (필수 필드)
  1. 빈 문자열: `Empty`
  2. 제어 문자: `ControlChar`
  3. 공백: `Whitespace`
  4. `-`로 시작: `LeadingDash`
  5. `@`, `#`, `"` 포함: `ForbiddenChar` (`@`는 유저를 User 필드에 넣도록 하기 위함)
- `validate_user` (선택 필드)
  1. 빈 문자열: `Ok`
  2. 제어 문자: `ControlChar`
  3. 공백: `Whitespace`
  4. `-`로 시작: `LeadingDash`
  5. `#`, `"` 포함: `ForbiddenChar`
- `parse_port` (선택 필드)
  - 빈 문자열: `Ok(None)`
  - ASCII 숫자만으로 이루어지고 값이 1-65535: `Ok(Some(n))`. 선행 0은 허용(`"022"`는 22).
  - 그 외(부호, 공백, 0, 65536 이상, 숫자 아닌 문자, 자릿수 과다로 인한 오버플로 포함): `InvalidPort`. 오버플로 입력(`"99999999999999999999"`)에서 패닉이 나면 안 된다.
- `validate_key_file` (선택 필드)
  1. 빈 문자열: `Ok`
  2. 제어 문자: `ControlChar`
  3. `"` 포함: `ForbiddenChar` (따옴표 처리 불가)
  4. `-`로 시작: `LeadingDash`
  - 공백은 허용한다(경로에 공백이 있을 수 있음).

테스트: 위 규칙마다 통과 사례와 거부 사례를 최소 1개씩 작성한다. 다음 사례는 반드시 포함한다.

- 닉네임: `web-1`, `db.prod_2`, `서버1` 통과. `""`, `-x`, `a b`, `a\tb`, `a*`, `a?b`, `!a`, `a#b`, `=a`, `a"b`, `a\nb` 거부. `a=b`와 `a-b` 통과.
- 주소: `example.com`, `192.168.0.1`, `::1`, `fe80::1%en0`, `xn--9t4b11yi5a.kr` 통과. `""`, `-oProxyCommand=x`, `a b`, `user@host`, `host\nProxyCommand x` 거부.
- 유저: `""`, `root`, `first.last`, `user@domain` 통과. `-l`, `a b`, `a\nb` 거부.
- 포트: `""`는 None, `1`, `22`, `65535`, `022` 통과. `0`, `65536`, `-1`, `+22`, `22 `, `2a`, `99999999999999999999` 거부.
- 키 파일: `""`, `~/.ssh/id_ed25519`, `/path/with space/key` 통과. `a"b`, `a\nb`, `-key` 거부.
- 중복: `["web", "db"]`에 대해 `web`은 `DuplicateNickname`, `Web`과 `we`는 통과.
- `Field`의 `Display` 출력 5종.

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- 위 테스트 목록이 모두 존재
