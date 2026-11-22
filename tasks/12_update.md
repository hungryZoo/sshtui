# 작업 12: 새 버전 확인과 업데이트

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `src/app/`, `src/ui/`, `src/tui.rs`, `src/system.rs`, `src/main.rs`, `src/ssh/args.rs`(`preview`)를 읽고 기존 구조에 맞춘다.

## 목표

새 버전 감지와 한 번에 업데이트를 추가한다. 설계 원칙은 다음과 같다.

- 시작 지연 0. 네트워크 응답을 기다리지 않는다.
- 확인은 하루 1회. 실행 횟수와 무관하다.
- 모달이나 질문 없는 알림. 목록 화면 제목 줄의 한 줄.
- 적용은 사용자의 키 입력(`u`, 이어서 `Enter`)으로 시작한다.
- 패키지 관리자 소유 바이너리는 그 관리자 명령으로만 갱신한다.
- 환경변수 `SSHTUI_NO_UPDATE_CHECK`로 완전히 끈다.

## 대상 파일

- `Cargo.toml`: `[package]`에 `repository = "https://github.com/hungryZoo/sshtui"` 추가, `[dependencies]`에 `dirs = "7.0.0"` 추가(알파벳 순서 유지). 이 두 가지 외 변경 금지. `dirs`와 그 의존성은 로컬 레지스트리에 내려받아 두었으므로 `--offline`으로 빌드된다. 빌드가 의존성 문제로 실패하면 우회하지 말고 최종 보고에 적는다.
- `src/update.rs` (신규), `src/lib.rs`(`pub mod update;`)
- `src/app/`(상태와 키 처리, 테스트 추가), `src/ui/`(알림과 모달, 테스트 추가)
- `src/system.rs`, `src/main.rs`

기존 테스트의 단언은 바꾸지 않는다. `Backend` trait에 메서드를 추가할 때는 기본 구현을 두어 기존 가짜 구현이 그대로 컴파일되게 한다.

## 확인 흐름 (4단계 파이프라인)

```
실행 ─┬─(1) 캐시 읽기 → 새 버전이면 알림 (즉시, 로컬 파일만)
      └─(2) 캐시가 24시간 경과? ─ 아니오 → 끝
                    │ 예
                    (3) checked_at=now 먼저 기록
                    (4) 백그라운드 스레드: curl 2초 → latest 기록
                         └ 결과는 "다음 실행"의 (1)에서 표시
```

- (1) 알림은 이전 실행이 남긴 캐시로만 그린다. 이번 실행의 네트워크 결과는 보지 않는다.
- (2) 주기 판정: `checked_at == 0` 또는 `now - checked_at >= 86400`.
- (3) 요청 전에 시도 시각을 먼저 기록한다. 기록에 실패하면 요청하지 않는다. 실패한 요청도 "오늘 시도함"으로 취급한다.
- (4) 스레드에서 `curl` 실행. 실패는 무시하고 성공 시에만 `latest`를 갱신한다. 스레드는 join하지 않는다.

최신 버전 조회는 GitHub 웹의 리다이렉트를 쓴다(API 미사용).

```
curl -fsSLI --max-time 2 -o <null 장치> -w %{url_effective} <REPOSITORY>/releases/latest
→ https://github.com/hungryZoo/sshtui/releases/tag/v1.2.3
```

- null 장치는 Unix `/dev/null`, Windows `NUL`.
- `stdin`과 `stderr`는 `Stdio::null()`. `curl`이 없거나 실패하면 조용히 `None`.
- URL의 `/tag/` 뒤를 잘라 `v`를 떼고 semver로 파싱한다. 파싱 실패는 `None`.
- HTTP 클라이언트 crate를 추가하지 않는다.

## src/update.rs

저장소 URL은 `env!("CARGO_PKG_REPOSITORY")` 한 곳에서만 가져온다. 바이너리와 패키지 이름은 `env!("CARGO_PKG_NAME")`을 쓴다. 문자열 상수로 `hungryZoo`나 `sshtui`를 코드에 직접 적지 않는다(테스트의 기대값은 예외).

```rust
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Environment variable that disables the check entirely.
pub const DISABLE_ENV: &str = "SSHTUI_NO_UPDATE_CHECK";

/// `<REPOSITORY>/releases/latest`
pub fn latest_url() -> String;
/// `<REPOSITORY>/releases/latest/download`
pub fn download_url() -> String;
/// `https://raw.githubusercontent.com/<owner>/<repo>/main/install.sh`, derived from REPOSITORY.
pub fn install_script_url() -> String;

/// Parses `MAJOR.MINOR.PATCH` with an optional leading `v`. Anything else (two parts,
/// pre-release suffixes, empty) is None.
pub fn parse_semver(text: &str) -> Option<(u32, u32, u32)>;
/// True only when both parse and `candidate > current`.
pub fn is_newer(candidate: &str, current: &str) -> bool;
/// Pure: a value that is non-empty and not "0" disables the check.
pub fn is_disabled_value(value: Option<&OsStr>) -> bool;
/// Reads DISABLE_ENV.
pub fn is_disabled() -> bool;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cache {
    /// Unix seconds of the last attempt (successful or not).
    pub checked_at: u64,
    /// Newest version seen upstream, without the `v`.
    pub latest: Option<String>,
}
impl Cache {
    /// Parses `key=value` lines; unknown keys, bad numbers and garbage fall back to defaults.
    pub fn parse(text: &str) -> Cache;
    /// `checked_at=<n>\nlatest=<v or empty>\n`
    pub fn render(&self) -> String;
    pub fn is_stale(&self, now: u64) -> bool;
}

/// `dirs::cache_dir()/<name>/update-check`; falls back to `<home>/.cache/...`, then `./.cache/...`.
pub fn cache_path() -> PathBuf;
pub fn read_cache(path: &Path) -> Cache;          // missing or unreadable -> default
pub fn write_cache(path: &Path, cache: &Cache) -> io::Result<()>;   // creates parent directories

/// Pure: extracts the version from the effective URL of the `/releases/latest` redirect.
pub fn version_from_release_url(url: &str) -> Option<String>;
/// Pure: curl arguments for the query (`null_device` is `/dev/null` or `NUL`).
pub fn fetch_args(url: &str, null_device: &str) -> Vec<OsString>;

/// Core of the background check, testable without network or the real cache:
/// when `cache` at `path` is stale for `now`, writes `checked_at = now` FIRST, then spawns a thread
/// that calls `fetch` and stores its answer as `latest`. Returns the thread handle, or None when the
/// cache is fresh or the attempt could not be recorded.
pub fn spawn_check_if_stale<F>(path: PathBuf, now: u64, fetch: F) -> Option<JoinHandle<()>>
where F: FnOnce() -> Option<String> + Send + 'static;

/// Real entry point: no-op when disabled, otherwise `spawn_check_if_stale(cache_path(), now, curl fetch)`.
/// The handle is dropped (never joined).
pub fn maybe_spawn_check();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    UpdateAvailable { latest: String },
    UpToDate { latest: String },
    Unreachable,
}
/// Core of `--check-update`, testable: sets `checked_at = now`, stores `latest` when fetched,
/// writes the cache (errors ignored) and classifies against `current`.
pub fn check_with(path: &Path, now: u64, current: &str, fetched: Option<String>) -> CheckOutcome;
/// Real entry point: fetches with curl, ignoring the 24 h interval.
pub fn check_now() -> CheckOutcome;
/// Human-readable report. Examples:
/// `sshtui 0.1.0 → 0.2.0 available\nupgrade: <hint>\nrelease: <latest_url>`
/// `sshtui 0.1.0 is up to date (latest release: 0.1.0)`
/// `sshtui 0.1.0: could not reach <latest_url> (offline, no release yet, or curl missing)`
pub fn report(outcome: &CheckOutcome, current: &str, method: &InstallMethod) -> String;
```

### 설치 방식 판정

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallMethod {
    Brew,
    Cargo,
    /// install.sh (or a hand-copied binary) in a user-writable directory.
    Script { bin_dir: PathBuf },
    Deb { arch: &'static str },
    Rpm { arch: &'static str },
    Unknown,
}

/// Debian and RPM architecture names for `std::env::consts::ARCH` values:
/// x86_64 -> (amd64, x86_64), aarch64 -> (arm64, aarch64), arm -> (armhf, no rpm), others None.
pub fn package_arch(arch: &str) -> Option<(&'static str, Option<&'static str>)>;

/// Pure classifier.
pub fn classify_install(
    exe: &Path,
    home: Option<&Path>,
    has_dpkg: bool,
    has_rpm: bool,
    arch: &str,
    windows: bool,
) -> InstallMethod;

/// Uses `current_exe()`, `dirs::home_dir()`, `/var/lib/dpkg/status`, `/var/lib/rpm`,
/// `std::env::consts::ARCH` and `cfg!(windows)`.
pub fn detect_install_method() -> InstallMethod;
```

판정 순서(위에서부터, 먼저 맞는 것):

1. `windows`가 true: `<home>/.cargo/bin` 아래면 `Cargo`, 아니면 `Unknown`. 아래 규칙은 적용하지 않는다.
2. 경로 문자열에 `/Cellar/`, `/homebrew/`, `/linuxbrew/` 포함: `Brew`. (Linuxbrew는 홈 아래에 있으므로 홈 검사보다 먼저다.)
3. `<home>/.cargo/bin` 아래: `Cargo`
4. 그 외 `<home>` 아래: `Script { bin_dir: 실행 파일의 디렉터리 }`
5. `/usr/bin/` 또는 `/usr/local/bin/` 아래이고 `package_arch`가 있을 때: `has_dpkg`이면 `Deb`, 아니고 `has_rpm`이며 rpm 아키텍처가 있으면 `Rpm`
6. 그 외 `Unknown`

### 업그레이드 명령

셸을 거치지 않는다(`sh -c` 금지). 명령은 인자 배열의 단계 목록으로 표현하고 순서대로 실행한다. 다운로드가 필요한 방식은 "파일로 내려받기"와 "실행"을 별도 단계로 둔다.

```rust
/// One process to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeStep { pub program: OsString, pub args: Vec<OsString> }

impl InstallMethod {
    /// Label for the modal: "Homebrew", "cargo", "install.sh in <dir>", "Debian package (.deb)",
    /// "RPM package", "unknown (manual install)".
    pub fn describe(&self) -> String;
    /// Short text for reports: the first step as a command line, or the release URL for Unknown.
    pub fn hint(&self, latest: &str) -> String;
    /// Steps that upgrade to `latest`; downloads go into `work_dir`. None for Unknown.
    pub fn upgrade_steps(&self, latest: &str, work_dir: &Path) -> Option<Vec<UpgradeStep>>;
    /// The same steps for display, one command line per entry, with `<tmp>` as the work directory.
    pub fn display_steps(&self, latest: &str) -> Vec<String>;
}
```

단계(`<name>`은 `CARGO_PKG_NAME`, `<latest>`는 `v` 없는 버전):

| 방식 | 단계 |
|---|---|
| `Brew` | `brew upgrade <name>` |
| `Cargo` | `cargo install --git <REPOSITORY> --tag v<latest> --locked` |
| `Script` | (1) `curl -fsSL -o <work_dir>/install.sh <install_script_url>` (2) `sh <work_dir>/install.sh --bin-dir <bin_dir>` |
| `Deb` | (1) `curl -fsSL -o <work_dir>/<name>_<arch>.deb <download_url>/<name>_<arch>.deb` (2) `sudo dpkg -i <work_dir>/<name>_<arch>.deb` |
| `Rpm` | `sudo rpm -U <download_url>/<name>-<arch>.rpm` |

- `Script`의 `--bin-dir`에는 원래 설치 디렉터리를 전달한다(PATH 앞쪽의 구버전이 남는 것을 방지).
- `upgrade_steps`에 넘기는 `latest`는 `parse_semver`를 통과한 값이어야 한다. 통과하지 못하면 `None`.
- 표시용 문자열은 `crate::ssh::args::preview`와 같은 인용 규칙으로 만든다(공용 함수로 분리해 재사용해도 된다. 분리하는 경우 `preview`의 기존 동작과 테스트는 유지).

```rust
/// What the notice, the modal and `--update` need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePlan { pub current: String, pub latest: String, pub method: InstallMethod }
impl UpdatePlan {
    /// One-line notice: `v<latest> available (u: update)`.
    pub fn notice(&self) -> String;
}

/// Pure: Some when `cache.latest` is newer than `current`.
pub fn plan_from(cache: &Cache, current: &str, method: InstallMethod) -> Option<UpdatePlan>;
/// Real entry point: None when disabled; otherwise reads the cache (no network).
pub fn plan() -> Option<UpdatePlan>;

/// Runs the steps in order with inherited stdio, in a fresh private temporary directory
/// (`tempfile::tempdir()`), printing `sshtui: running: <command line>` before each step.
/// Stops at the first failing step. Ok(true) when every step succeeded.
/// The caller must have restored the terminal first.
pub fn run_upgrade(plan: &UpdatePlan) -> io::Result<bool>;
```

임시 디렉터리를 고정 경로(`/tmp/sshtui-update` 등)로 두지 않는다. 내려받은 스크립트와 패키지를 실행하므로 다른 사용자가 미리 만들 수 없는 `tempfile::tempdir()`을 쓴다.

## 화면 연동

### Backend와 App

- `Backend`에 `fn update_plan(&self) -> Option<UpdatePlan> { None }`(기본 구현) 추가.
- `SystemBackend`에 `pub fn with_update_plan(self, plan: Option<UpdatePlan>) -> Self`를 추가하고 `update_plan()`이 그 값을 돌려주게 한다.
- `App::new`가 `backend.update_plan()`을 한 번 읽어 보관한다. 접근자 `pub fn update_plan(&self) -> Option<&UpdatePlan>`.
- `Mode::Update(UpdateState)` 추가. 접근자: 현재 버전, 최신 버전, 설치 방식 설명, 표시용 단계 목록, 실행 가능 여부(`Unknown`이면 false), 릴리스 URL.
- `Exit::Update(UpdatePlan)` 추가.

키:

| 화면 | 키 | 동작 |
|---|---|---|
| 목록(필터 입력 중 아님) | `u` | `update_plan`이 있으면 `Mode::Update`. 없으면 무시 |
| 목록(필터 입력 중) | `u` | 글자(필터에 입력) |
| Update | `Enter` | 실행 가능하면 `Exit::Update(plan)`. `Unknown`이면 아무 일도 없음 |
| Update | `Esc` | 목록으로 복귀 |

TUI 안에서는 명령을 실행하지 않는다(raw mode와 alternate screen에서는 `sudo` 프롬프트와 설치 출력이 깨진다).

### 렌더링

- 목록 화면 제목 줄의 오른쪽 끝에 `UpdatePlan::notice()`를 노랑 굵게 표시한다. 제목 텍스트와 겹치지 않을 폭이 있을 때만 그린다(폭이 부족하면 생략). 상태 메시지와 키 안내 줄은 건드리지 않는다. 굵게 표시가 옆 텍스트로 번지지 않게 한다.
- 목록 화면의 키 안내에 `update_plan`이 있을 때만 `u update`를 추가한다(`q quit` 앞).
- Update 모달(팝업): 제목 `Update sshtui`, 본문 `Current: <현재>`, `Latest: <최신>`, `Installed via: <설명>`, 실행 가능하면 `Command:` 다음 줄부터 표시용 단계를 한 줄씩, 불가능하면 `Download: <릴리스 URL>`. 긴 줄은 줄바꿈. 키 안내는 실행 가능하면 `Enter update and exit  Esc cancel`, 아니면 `Esc close`.
- 작은 터미널에서 패닉 없이 그린다.

### main

- `parse_args`에 `--check-update`와 `--update` 추가(`CliAction::CheckUpdate`, `CliAction::Update`). 단독 인자로만 받는다. `--help`에 두 옵션과 `SSHTUI_NO_UPDATE_CHECK` 설명을 추가한다.
- `--check-update`: `check_now()` 결과를 `report`로 출력. 종료 코드 0(최신), 10(업데이트 있음), 11(접속 불가).
- `--update`: `check_now()` 결과를 출력하고, 업데이트가 있으면 `detect_install_method()`로 계획을 만들어 `run_upgrade`. 종료 코드 0(완료 또는 최신), 1(실패 또는 설치 방식 불명), 11(접속 불가). 불명일 때는 표준 오류에 수동 갱신 안내와 릴리스 URL을 출력한다.
- 두 플래그는 `SSHTUI_NO_UPDATE_CHECK`와 무관하게 동작한다(사용자가 명시적으로 요청한 확인이므로).
- TUI 실행(`CliAction::Run`): `find_ssh`와 터미널 점검을 통과한 뒤 `let plan = update::plan(); update::maybe_spawn_check();`를 호출하고 `SystemBackend`에 `with_update_plan(plan)`으로 전달한다. 이 두 호출은 네트워크 응답을 기다리지 않는다.
- `Exit::Update(plan)`: 터미널 복원 뒤 `run_upgrade`. 성공하면 `sshtui: updated. Start sshtui again to use the new version.` 출력 후 종료 코드 0. 실패하면 표준 오류에 사유 출력 후 종료 코드 1. 같은 프로세스로 TUI를 다시 그리지 않는다.

## 테스트

네트워크와 실제 캐시 경로를 쓰는 테스트는 금지한다. `curl` 실행, `check_now()`, `maybe_spawn_check()`, `plan()`, `cache_path()`에 쓰기, `run_upgrade()`, `detect_install_method()` 결과에 의존하는 단언을 테스트에서 하지 않는다. 캐시 파일 테스트는 `tempfile::tempdir()` 안에서만 한다.

update.rs:

1. `parse_semver`와 `is_newer`: `1.2.3`, `v1.2.3`, `1.2`(None), `1.2.3-rc1`(None), 빈 문자열, 비교 4가지, 파싱 불가 입력은 새 버전 아님
2. `version_from_release_url`: `.../releases/tag/v1.1.1`은 `1.1.1`, `.../releases/latest`와 `.../releases`와 빈 문자열은 None, `v` 없는 태그 `1.2.3`도 허용, `15.2.0-beta`는 None
3. `fetch_args`: `["-fsSLI", "--max-time", "2", "-o", "/dev/null", "-w", "%{url_effective}", <url>]`와 완전 일치
4. `Cache` 왕복, 빈 입력, `latest=` 빈 값, 손상된 입력(`checked_at=abc`, 임의 문자열)은 기본값, `is_stale` 경계(0, 24시간 직전, 정확히 24시간)
5. 캐시 파일 왕복(중첩 디렉터리 생성), 없는 파일은 기본값
6. `spawn_check_if_stale`: 오래된 캐시에서 `fetch`가 불리는 시점에 파일의 `checked_at`이 이미 `now`임(클로저 안에서 파일을 읽어 확인), join 후 `latest` 기록됨
7. `spawn_check_if_stale`: `fetch`가 `None`을 돌려줘도 `checked_at`은 `now`로 남고 기존 `latest`는 유지
8. `spawn_check_if_stale`: 신선한 캐시면 `None`을 반환하고 `fetch`가 불리지 않으며 파일 무변화
9. `check_with`: 새 버전, 같은 버전, 조회 실패 각각의 결과와 캐시 기록
10. `is_disabled_value`: None, 빈 값, `0`, `1`, `true`
11. `classify_install`: `/opt/homebrew/bin/x`, `/usr/local/Cellar/x/1.0/bin/x`, `/home/linuxbrew/.linuxbrew/bin/x`(Brew), `<home>/.cargo/bin/x`(Cargo), `<home>/.local/bin/x`(Script와 bin_dir), `/usr/bin/x`와 dpkg(Deb amd64), `/usr/bin/x`와 rpm(Rpm x86_64), 둘 다 없음(Unknown), aarch64 deb(arm64), arm과 rpm(Unknown), home 없음과 알 수 없는 경로(Unknown), `windows = true`에서 cargo 경로는 Cargo이고 그 외는 Unknown
12. `upgrade_steps`: 방식별 단계의 프로그램과 인자 완전 일치(`work_dir`는 임의 경로). `Script`에서 공백이 든 `bin_dir`이 인자 하나로 유지됨. `Unknown`과 파싱 불가 `latest`는 None
13. `display_steps`: `<tmp>` 표기, 공백이 든 경로의 인용
14. URL 함수 3종의 값이 `REPOSITORY`에서 파생됨(`https://github.com/hungryZoo/sshtui/releases/latest` 등)
15. `plan_from`: 새 버전이면 Some, 같거나 낮거나 `latest` 없음이면 None. `notice()` 문자열
16. `report` 3가지 결과

app:

17. `update_plan`이 없으면 `u`는 무시(모드 유지), 있으면 `Mode::Update`
18. Update에서 `Esc`는 목록 복귀, `Enter`는 `Exit::Update(plan)`
19. `Unknown` 방식이면 `Enter`를 눌러도 종료하지 않음
20. 필터 입력 중 `u`는 필터 글자
21. 폼에서 `u`는 글자(모드 유지)

ui:

22. 목록 제목 줄에 알림 문구와 키 안내의 `u update` 표시, 계획이 없으면 둘 다 없음
23. 폭이 좁으면(예: 30) 알림이 생략되고 패닉 없음
24. Update 모달: 버전 2개, 설치 방식 설명, 단계 줄, 키 안내. `Unknown`이면 `Download:`와 URL, `Esc close`
25. 작은 크기(1x1, 5x2, 20x5)에서 Update 모달이 패닉 없이 그려짐

main:

26. `parse_args`: `--check-update`, `--update`, 뒤에 인자가 더 붙으면 `Invalid`

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- `cargo check --offline --target x86_64-pc-windows-msvc --all-targets`와 `cargo check --offline --target x86_64-unknown-linux-gnu --all-targets` 통과
- 위 테스트가 모두 존재하고 기존 테스트 280개 유지
- `src/update.rs`에 `sh -c` 등 셸 경유 실행 없음
