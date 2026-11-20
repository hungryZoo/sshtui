# 작업 10: TUI 렌더링, 이벤트 루프, 실행 연결

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `src/app/`(상태와 `Backend` trait), `src/dialog/`, `src/ssh/`, `src/config/`, `src/keys.rs`, `src/paths.rs`를 읽고 그 타입을 사용한다.

## 목표

1. `system`: `Backend`의 실제 구현(`SystemBackend`)
2. `ui`: `App` 상태를 ratatui로 그리는 렌더러(입출력 없는 함수)
3. `tui`: 터미널 초기화와 복원, 이벤트 루프
4. `main`: 인자 처리, 사전 점검, TUI 실행, 종료 후 ssh 실행

## 대상 파일

- `src/system.rs` (신규), `src/lib.rs`(`pub mod system;` 추가)
- `src/ui/mod.rs`와 `src/ui/` 아래 신규 파일
- `src/tui.rs`
- `src/main.rs`
- `src/app/`: 렌더러에 필요한 읽기 전용 접근자 추가만 허용(예: `NoticeState`가 접속으로 이어지는지 여부). 상태 전이 로직과 기존 테스트는 수정하지 않는다.

ratatui 0.30.2와 crossterm 0.29.0의 API는 `~/.cargo/registry/src/*/ratatui-0.30.2/`, `ratatui-core-0.1.2/`, `ratatui-widgets-0.3.2/`, `crossterm-0.29.0/`에서 확인한다. crossterm 타입은 직접 의존성 `crossterm`에서 가져온다.

## src/system.rs

```rust
/// Backend that talks to the real file system, ssh and the native dialog.
pub struct SystemBackend { /* paths: SshPaths, ssh: PathBuf, picker: Box<dyn FilePicker> */ }
impl SystemBackend {
    pub fn new(paths: SshPaths, ssh: PathBuf, picker: Box<dyn FilePicker>) -> Self;
    /// Path of the ssh executable.
    pub fn ssh(&self) -> &Path;
}
impl Backend for SystemBackend { ... }
```

- `config_prefix`: `config_file_args(&paths, paths.config_file().exists())`
- `load_config`: `config::load(&paths)`, 오류는 `to_string()`
- `effective`: `SystemResolver::new(ssh.clone(), self.config_prefix()).resolve(alias)`, 오류는 `to_string()`
- `discover_keys`: `keys::discover_keys(paths.ssh_dir())`, 오류는 빈 목록
- `is_writable`: `config::writer::is_writable`
- `commit`: `pending.commit()`
- `picker_unavailable_reason`, `hidden_files_hint`: picker에 위임
- `pick_file`: `picker.pick_file(paths.ssh_dir())`

테스트(임시 디렉터리, 가짜 `FilePicker`, ssh 경로는 임시 디렉터리의 가짜 스크립트 또는 `find_ssh()` 결과):

1. 주입된 ssh 디렉터리의 config를 `load_config`로 읽음
2. `config_prefix`: config가 있으면 `["-F", <config>]`, 없으면 `["-F", "none"]`
3. `commit`으로 실제 임시 파일이 바뀌고 백업 생성
4. `discover_keys`가 임시 디렉터리의 가짜 키를 찾음, 디렉터리가 없으면 빈 목록
5. `pick_file`이 ssh 디렉터리를 시작 위치로 가짜 picker에 전달
6. `find_ssh()`가 성공하는 환경에서만: `effective("web")`이 임시 config의 값을 반환(`-F` 사용). ssh가 없으면 조용히 통과

## src/ui (렌더러)

```rust
/// Draws the whole screen for the current application state. No I/O.
pub fn render<B: Backend>(frame: &mut ratatui::Frame, app: &App<B>);
```

공통 배치: 맨 위 제목 1줄, 가운데 본문, 아래에서 둘째 줄 상태 메시지(Info는 기본색, Warning은 노랑, Error는 빨강), 맨 아래 줄 키 안내. 터미널이 아주 작아도(예: 1x1, 10x3) 패닉 없이 그릴 수 있는 만큼만 그린다. 영역 계산에서 뺄셈 언더플로가 없어야 한다.

### 목록 화면

- 제목: `sshtui` 와 호스트 수(필터 적용 시 `3 of 12 hosts` 형태)
- 왼쪽: 호스트 이름 목록. 선택 항목 강조. 읽기 전용 항목에는 `[ro]` 표시. 선택 항목이 항상 보이도록 스크롤(`ListState` 사용). 필터 결과가 없으면 `No matching hosts`
- 오른쪽(상세): 선택 항목의 `Alias`, `User`, `Address`, `Port`, `Key file`(여러 개면 줄마다 하나), `Source`(`<파일 경로>:<줄 번호>`, 홈 하위는 `abbreviate_home` 적용), 읽기 전용이면 `Read-only file`. 적용값 조회가 실패했으면 오류 문구
- 필터 입력 중이거나 필터가 있으면 필터 줄 `/<필터>` 표시, 입력 중이면 커서 위치 지정
- 키 안내: `Enter connect  n new  e edit  d delete  / filter  r reload  q quit`

### 폼 화면

- 제목: `New connection` 또는 `Edit <alias>`
- 필드 5개를 순서대로: 라벨(`Nickname`, `User`, `Address`, `Port`, `Key file`)과 값. 새 접속 폼의 라벨에는 선택 또는 필수 표시(`Nickname (optional)`, `Address (required)` 등)
  - 값이 비어 있고 placeholder가 있으면 흐리게 `inherited: <값>` 표시
  - 읽기 전용 필드는 원문 줄을 흐리게 표시하고 `read-only` 표시
  - 실시간 검증 오류가 있으면 필드 옆 또는 아래에 빨강으로 표시
  - 포커스 필드는 강조하고 `frame.set_cursor_position`으로 커서를 `cursor_column` 위치에 둔다. 값이 영역보다 길면 커서가 보이도록 가로 스크롤한다.
- 공유 알림(`shared_notice`)이 있으면 노랑으로 표시
- 키 파일 안내 줄: `Ctrl+L / F2: pick from ~/.ssh   Ctrl+O / F3: browse`. 찾아보기 비활성 사유가 있으면 browse 부분 대신 그 사유를 흐리게 표시. `hidden_files_hint`가 있으면 그 문구도 표시
- 미리보기 영역: `Command: ssh ...` 또는 오류일 때 `Command: (<오류>)`
- 키 안내: 새 접속은 `Tab/Shift+Tab move  Enter connect  Esc back`, 편집은 `Tab/Shift+Tab move  Enter save  Esc back`

### 확인 화면

- 제목(`ConfirmState::title`), 대상 파일 경로
- diff: hunk 사이에 구분선, 줄마다 줄 번호(old, new)와 기호(`-` 빨강, `+` 초록, 공백 흐림), 스크롤 위치 반영
- 키 안내: `y/Enter confirm  n/Esc cancel  Up/Down scroll`

### 키 목록(폼 위 팝업)

- `KeyInfo::label()` 목록, 선택 강조. 키 안내: `Enter select  Esc cancel`

### 알림(팝업)

- 제목과 본문 줄. 접속으로 이어지는 알림이면 `Enter connect anyway  Esc back to list`, 아니면 `Enter/Esc back to list`

### 렌더러 테스트

`ratatui::backend::TestBackend`와 `Terminal`로 그린 뒤 버퍼의 텍스트를 검사한다. 상태는 `src/app/tests.rs`와 같은 방식의 가짜 `Backend`로 만든다(테스트 지원 코드를 `ui` 테스트 모듈에 다시 작성해도 되고, `src/app/`에 `#[cfg(test)] pub(crate)` 지원 모듈로 옮겨 공유해도 된다. 옮기는 경우 기존 테스트의 단언은 바꾸지 않는다).

7. 목록: 호스트 이름들, 선택 항목의 상세(User, Address, Port, Key file, Source와 줄 번호), `[ro]` 표시, 키 안내 문구
8. 목록: 필터 줄과 `No matching hosts`, 제목의 `1 of 3 hosts`
9. 목록: 적용값 조회 실패 시 오류 문구
10. 폼(새 접속): 라벨 5개, 미리보기 `Command: ssh ...`, 주소가 비었을 때 미리보기 오류, 키 안내
11. 폼(편집): 값, `inherited: ...` placeholder, 읽기 전용 원문과 `read-only`, 공유 알림
12. 폼: 찾아보기 비활성 사유 표시, 숨김파일 안내 표시
13. 폼: 검증 오류 문구 표시, 커서 위치가 포커스 필드 값 끝(한글 입력 포함)에 있음
14. 확인: 파일 경로, `-`와 `+` 줄의 내용, 키 안내
15. 키 목록 팝업과 알림 팝업
16. 상태 메시지 표시
17. 작은 크기(1x1, 5x2, 20x5, 40x10)에서 모든 화면 종류가 패닉 없이 그려짐
18. 호스트가 화면 높이보다 많을 때 마지막 항목을 선택하면 그 항목이 보임

## src/tui.rs

```rust
/// Runs the event loop until the application requests an exit. The terminal is restored
/// before this function returns, on success, error and panic alike.
pub fn run<B: Backend>(app: App<B>) -> std::io::Result<Exit>;
```

- 초기화: raw mode, alternate screen, bracketed paste 활성화. 복원: bracketed paste 비활성화, alternate screen 종료, raw mode 해제, 커서 표시.
- 복원은 `Drop` 가드로 보장한다(정상 종료, `?`에 의한 조기 반환, 패닉 unwind 모두). 추가로 패닉 훅을 설치해 패닉 메시지가 출력되기 전에 터미널을 복원하고 기존 훅을 이어서 호출한다. ratatui 0.30의 `ratatui::init`/`ratatui::restore`가 이를 제공하면 사용해도 된다. 단, bracketed paste의 활성화와 비활성화, 패닉 시 비활성화가 빠지지 않아야 한다.
- 루프: `terminal.draw(|frame| ui::render(frame, &app))` 후 `crossterm::event::read()`(블로킹). `Event::Key`는 `app.handle_key`, `Event::Paste`는 `app.handle_paste`, `Event::Resize`는 다시 그리기. 이벤트 처리 후 `app.take_full_redraw()`가 true이면 `terminal.clear()`. `app.exit()`가 있으면 루프 종료.
- 반환 전 터미널을 복원한 뒤 `Exit`를 돌려준다. ssh 실행은 이 함수 밖(`main`)에서 한다.

이벤트 루프 자체는 단위 테스트를 작성하지 않는다. 대신 이벤트 한 건을 처리하는 부분을 `fn handle_event<B: Backend>(app: &mut App<B>, event: Event)`로 분리하고 이 함수의 테스트를 작성한다.

19. `handle_event`: Key, Paste 전달과 Resize, 포커스 이벤트 등 나머지 무시

## src/main.rs

기존 `parse_args`와 그 테스트 4개는 유지한다. `CliAction::Run` 처리:

1. `SshPaths::from_env()`. 실패하면 표준 오류에 `sshtui: <오류>` 출력, 종료 코드 1.
2. `ssh::exec::find_ssh()`. 실패하면 `sshtui: <오류>`(PATH에 ssh가 없다는 문구) 출력, 종료 코드 1. TUI에 들어가기 전에 수행한다.
3. 표준 입력과 표준 출력이 터미널인지 확인(`std::io::IsTerminal`). 아니면 `sshtui: an interactive terminal is required` 출력, 종료 코드 1.
4. `SystemBackend::new(paths, ssh, dialog::system_picker())`, `App::new`, `tui::run`.
5. `tui::run`이 오류면 `sshtui: <오류>` 출력, 종료 코드 1.
6. `Exit::Quit`이면 종료 코드 0.
7. `Exit::Connect(args)`이면 `ssh::exec::run_interactive(&ssh, &args)`. Unix에서는 프로세스가 교체되므로 반환되면 오류이고 `sshtui: <오류>` 출력 후 종료 코드 1. Windows에서는 반환된 종료 코드를 그대로 프로세스 종료 코드로 사용한다(0-255 범위로 변환, 범위를 벗어나면 1).

도움말(`--help`)에 환경변수 `SSHTUI_SSH_DIR` 한 줄 설명을 추가한다.

`main`의 2-7 단계는 `fn run() -> ExitCode` 등으로 나누되 단위 테스트는 요구하지 않는다.

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- `cargo check --offline --target x86_64-pc-windows-msvc --all-targets`와 `cargo check --offline --target x86_64-unknown-linux-gnu --all-targets` 통과
- `cargo build --offline` 성공
- 위 테스트가 모두 존재하고 기존 테스트 251개 유지
- `src/ui/`에 파일 시스템, 프로세스, 환경변수 접근 없음(테스트 모듈 제외)
