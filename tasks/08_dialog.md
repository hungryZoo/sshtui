# 작업 8: 네이티브 파일 선택 다이얼로그

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 목표

키 파일을 고르는 OS 네이티브 "파일 열기" 다이얼로그를 trait 뒤에 구현한다. 핵심 요구사항은 숨김파일이 보이는 상태로 열리는 것이다. 강제할 수 없는 플랫폼은 사용자가 켜는 방법을 안내 문구로 제공한다.

## 대상 파일

- `src/dialog/mod.rs`
- `src/dialog/command.rs` (신규: 외부 명령 기반 구현과 순수 함수. 모든 OS에서 컴파일)
- `src/dialog/windows.rs` (신규: `#[cfg(windows)]`에서만 컴파일)

## 플랫폼별 방식 (변경 금지)

| OS | 수단 | 숨김파일 |
|---|---|---|
| macOS | `osascript` + AppleScript `choose file ... with invisibles` | 강제 표시 |
| Windows | `rfd` 0.18 `FileDialog::set_show_hidden_files(true)` (내부적으로 `FOS_FORCESHOWHIDDEN`) | 강제 표시 |
| Linux과 그 외 Unix | `zenity --file-selection` 또는 `kdialog --getopenfilename` | 강제 불가, 안내 문구 |

- Linux에서 `rfd`를 쓰지 않는다(숨김파일 옵션이 GTK3 백엔드 전용이고 XDG portal에는 옵션이 없음).
- `rfd`는 `Cargo.toml`에 이미 Windows 전용 의존성으로 있다. API는 `~/.cargo/registry/src/*/rfd-0.18.0/src/file_dialog.rs`에서 확인한다.

## src/dialog/mod.rs

```rust
pub mod command;
#[cfg(windows)]
pub mod windows;

/// Result of showing the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome { Picked(PathBuf), Cancelled }

#[derive(Debug, thiserror::Error)]
pub enum DialogError {
    /// The picker cannot be shown in this session; the string explains why.
    #[error("{0}")]
    Unavailable(String),
    /// The picker was started but failed.
    #[error("file dialog failed: {0}")]
    Failed(String),
}

/// A native "open file" dialog.
pub trait FilePicker {
    /// Why "Browse" is disabled, or None when the dialog can be shown.
    fn unavailable_reason(&self) -> Option<String>;
    /// Instructions for showing hidden files when the platform cannot force them.
    fn hidden_files_hint(&self) -> Option<String>;
    /// Shows the dialog starting in `start_dir` and blocks until it closes.
    fn pick_file(&self, start_dir: &Path) -> Result<PickOutcome, DialogError>;
}

/// Operating system family used to choose a picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform { MacOs, Windows, OtherUnix }
impl Platform { pub fn current() -> Self; }

/// Snapshot of the environment facts that decide availability (pure input for tests).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionEnv {
    /// Any of SSH_CONNECTION, SSH_CLIENT, SSH_TTY is set and non-empty.
    pub over_ssh: bool,
    /// DISPLAY is set and non-empty.
    pub display: bool,
    /// WAYLAND_DISPLAY is set and non-empty.
    pub wayland_display: bool,
    /// XDG_CURRENT_DESKTOP value, if any.
    pub current_desktop: Option<String>,
    /// zenity was found on PATH.
    pub has_zenity: bool,
    /// kdialog was found on PATH.
    pub has_kdialog: bool,
}
impl SessionEnv {
    /// Reads the process environment and looks up zenity and kdialog with `which`.
    pub fn from_process() -> Self;
}

/// Pure: why a GUI dialog cannot be shown, or None.
pub fn gui_unavailable_reason(platform: Platform, env: &SessionEnv) -> Option<String>;

/// Builds the picker for the current platform.
pub fn system_picker() -> Box<dyn FilePicker>;
```

`gui_unavailable_reason` 규칙(위에서부터 먼저 해당하는 것):

1. `over_ssh`: `"Browse is unavailable over SSH (no local desktop session)"` (모든 플랫폼)
2. `OtherUnix`이고 `display`와 `wayland_display`가 모두 false: `"Browse is unavailable: no graphical session (DISPLAY and WAYLAND_DISPLAY are not set)"`
3. `OtherUnix`이고 `has_zenity`와 `has_kdialog`가 모두 false: `"Browse is unavailable: install zenity or kdialog"`
4. 그 외 `None`

`system_picker`: `Platform::current()`와 `SessionEnv::from_process()`로 아래를 만든다.

- `MacOs`: `command::CommandPicker` (osascript)
- `OtherUnix`: `command::CommandPicker` (zenity 또는 kdialog)
- `Windows`: `windows::RfdPicker`

## src/dialog/command.rs

외부 명령을 실행하고 표준 출력에서 경로를 읽는 구현이다. 명령과 인자를 만드는 부분은 순수 함수로 두어 모든 OS에서 테스트한다.

```rust
/// Which external tool shows the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool { Osascript, Zenity, Kdialog }

/// A program and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation { pub program: OsString, pub args: Vec<OsString> }

/// AppleScript source used on macOS. The start directory is passed as an argument
/// (`on run argv`), never interpolated into the script.
pub const OSASCRIPT_SOURCE: &str = /* see below */;

/// Pure: tool choice for non-macOS Unix. KDE desktops prefer kdialog, others prefer zenity;
/// falls back to whichever is installed. None when neither is installed.
pub fn choose_unix_tool(env: &SessionEnv) -> Option<Tool>;

/// Pure: command line for `tool` starting in `start_dir`.
pub fn invocation(tool: Tool, start_dir: &Path) -> Invocation;

/// Pure: interprets the tool's result. A failed status or empty output is `Cancelled`.
/// One trailing `\n` or `\r\n` is removed; other whitespace is kept.
pub fn parse_output(success: bool, stdout: &[u8]) -> PickOutcome;

/// Pure: hint for showing hidden files, or None when the tool forces them.
pub fn hidden_files_hint(tool: Tool) -> Option<&'static str>;

/// Nearest existing directory at or above `dir` (file system access). Falls back to `dir` itself.
pub fn existing_start_dir(dir: &Path) -> PathBuf;

/// Picker backed by an external command.
#[derive(Debug, Clone)]
pub struct CommandPicker { /* tool: Option<Tool>, unavailable: Option<String> */ }
impl CommandPicker {
    pub fn new(tool: Option<Tool>, unavailable: Option<String>) -> Self;
}
impl FilePicker for CommandPicker { ... }
```

AppleScript(`OSASCRIPT_SOURCE`)는 아래 그대로 사용한다.

```applescript
on run argv
	set startDir to POSIX file (item 1 of argv)
	try
		set chosen to choose file with prompt "Select an SSH private key" default location startDir with invisibles
		return POSIX path of chosen
	on error number -128
		return ""
	end try
end run
```

`invocation` 규칙(`<dir>`은 `start_dir`의 문자열):

- `Osascript`: `osascript`, `["-e", OSASCRIPT_SOURCE, "<dir>"]`
- `Zenity`: `zenity`, `["--file-selection", "--title=Select an SSH private key", "--filename=<dir>/"]` (끝에 `/`를 붙여 디렉터리로 열리게 함. `<dir>`이 이미 `/`로 끝나면 추가하지 않음)
- `Kdialog`: `kdialog`, `["--title", "Select an SSH private key", "--getopenfilename", "<dir>"]`

`choose_unix_tool` 규칙: `current_desktop`에 대소문자 무시로 `KDE`가 들어 있고 `has_kdialog`이면 `Kdialog`. 아니면 `has_zenity`이면 `Zenity`, 그다음 `has_kdialog`이면 `Kdialog`, 둘 다 없으면 `None`.

`hidden_files_hint`: `Osascript`는 `None`, `Zenity`는 `"Hidden files: press Ctrl+H in the dialog"`, `Kdialog`는 `"Hidden files: press Alt+. in the dialog"`.

`CommandPicker` 동작:

- `unavailable_reason`: 생성 시 받은 `unavailable`. `tool`이 `None`이면 `"Browse is unavailable: no supported dialog tool found"`.
- `hidden_files_hint`: `tool`에 대한 `hidden_files_hint`.
- `pick_file`: `unavailable_reason()`이 있으면 실행하지 않고 `DialogError::Unavailable`. 아니면 `invocation(tool, &existing_start_dir(start_dir))`를 `std::process::Command`로 실행한다. `stdin`은 `Stdio::null()`, `stdout`은 파이프, `stderr`는 `Stdio::null()`(다이얼로그 도구의 경고가 TUI 화면을 깨뜨리지 않게 함). 실행 자체가 실패하면 `DialogError::Failed`. 결과는 `parse_output(status.success(), &stdout)`.
- 셸을 거치지 않는다.

테스트용으로 임의 프로그램을 실행할 수 있도록, `pick_file`의 실행 부분을 `fn run_invocation(invocation: &Invocation) -> Result<PickOutcome, DialogError>`(pub)로 분리한다.

## src/dialog/windows.rs (`#[cfg(windows)]`)

```rust
/// Picker backed by the Windows common item dialog through `rfd`.
#[derive(Debug, Clone)]
pub struct RfdPicker { /* unavailable: Option<String> */ }
impl RfdPicker { pub fn new(unavailable: Option<String>) -> Self; }
impl FilePicker for RfdPicker { ... }
```

- `pick_file`: `rfd::FileDialog::new().set_title("Select an SSH private key").set_directory(existing_start_dir(start_dir)).set_show_hidden_files(true).pick_file()`. `None`은 `Cancelled`.
- 호출 스레드에서 동기 호출한다(창 없는 앱은 메인 스레드 동기 호출만 가능).
- `hidden_files_hint`는 `None`.

이 파일은 현재 머신(macOS)에서 실행 검증이 불가능하다. `cargo check --offline --target x86_64-pc-windows-msvc`로 타입 검사만 통과시킨다.

## 테스트

실제 다이얼로그를 띄우는 테스트는 금지한다(`osascript`, `zenity`, `kdialog`, `rfd` 실행 금지). `system_picker().pick_file(...)`도 테스트에서 호출하지 않는다.

1. `gui_unavailable_reason`: SSH 접속 중(3개 플랫폼 모두), `OtherUnix`에서 DISPLAY와 WAYLAND_DISPLAY 없음, `OtherUnix`에서 도구 없음, `OtherUnix`에서 WAYLAND_DISPLAY만 있고 zenity 있음(None), `MacOs`와 `Windows`는 DISPLAY 없이도 None, 우선순위(SSH가 먼저)
2. `choose_unix_tool`: KDE 데스크톱과 둘 다 설치(kdialog), GNOME과 둘 다 설치(zenity), KDE인데 zenity만 설치(zenity), kdialog만 설치(kdialog), 둘 다 없음(None), `current_desktop`이 `ubuntu:GNOME`이나 `KDE` 소문자 표기인 경우
3. `invocation` 3종의 프로그램과 인자 완전 일치. 공백이 든 디렉터리가 인자 하나로 유지됨. zenity의 끝 `/` 처리 2가지
4. `OSASCRIPT_SOURCE`에 `with invisibles`와 `default location`이 들어 있고 디렉터리 경로가 스크립트에 삽입되지 않음(`invocation` 결과의 스크립트 인자가 상수와 동일)
5. `parse_output`: 정상 경로, 끝 줄바꿈 제거(`\n`, `\r\n`), 빈 출력은 Cancelled, 실패 상태는 Cancelled, 공백이 든 경로 유지
6. `hidden_files_hint` 3종
7. `existing_start_dir`: 존재하는 디렉터리는 그대로, 없는 하위 경로는 존재하는 조상, (임시 디렉터리 사용)
8. `CommandPicker`: `unavailable`이 있으면 `pick_file`이 실행 없이 `Unavailable`, `tool`이 None이면 사유 반환
9. (`#[cfg(unix)]`) `run_invocation`: 임시 디렉터리의 가짜 셸 스크립트(`#!/bin/sh`)로 경로 출력 시 `Picked`, 종료 코드 1이면 `Cancelled`, 없는 프로그램이면 `Failed`
10. 테스트 모듈 안의 가짜 `FilePicker` 구현을 `Box<dyn FilePicker>`로 사용 가능함을 확인

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- `cargo check --offline --target x86_64-pc-windows-msvc --all-targets`와 `cargo check --offline --target x86_64-unknown-linux-gnu --all-targets` 통과
- 위 테스트가 모두 존재하고 기존 테스트 유지
