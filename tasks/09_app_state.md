# 작업 9: 화면 상태 (app)

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 기존 모듈을 읽고 그 타입을 그대로 사용한다: `config`(`ConfigSet`, `HostEntry`, `BlockFields`, `FieldState`, `Change`, `OptionChanges`, `NewHost`, `plan_add`, `plan_edit`, `plan_delete`, `PendingWrite`, `DiffHunk`, `WriteError`), `keys`(`KeyInfo`), `ssh`(`ConnectTarget`, `connect_args`, `preview`, `EffectiveConfig`, `ExpectedConfig`, `find_mismatches`), `dialog`(`PickOutcome`, `DialogError`), `validate`, `paths`.

## 목표

TUI의 화면 상태와 키 입력에 따른 상태 전이를 구현한다. 렌더링과 터미널 제어는 다음 작업(10)의 범위다. 이 작업의 코드는 터미널, 파일 시스템, 프로세스에 직접 접근하지 않고 모든 부수 효과를 `Backend` trait을 통해 수행한다. 테스트는 가짜 `Backend`로 작성한다.

## 대상 파일

- `src/app/mod.rs`와 `src/app/` 아래 신규 파일(권장 구성: `backend.rs`, `input.rs`, `list.rs`, `form.rs`, `confirm.rs`). 파일 구성은 조정 가능하다.
- 다른 모듈은 수정하지 않는다. 기존 모듈에 부족한 점이 있으면 수정하지 말고 최종 보고에 적는다.

## Backend trait

```rust
/// Every side effect the application state needs.
pub trait Backend {
    fn paths(&self) -> &SshPaths;
    /// `-F` arguments for ssh (see `ssh::args::config_file_args`), evaluated at call time.
    fn config_prefix(&self) -> Vec<OsString>;
    fn load_config(&self) -> Result<ConfigSet, String>;
    fn effective(&self, alias: &str) -> Result<EffectiveConfig, String>;
    fn discover_keys(&self) -> Vec<KeyInfo>;
    fn is_writable(&self, path: &Path) -> bool;
    fn commit(&self, pending: &PendingWrite) -> Result<WriteOutcome, WriteError>;
    fn picker_unavailable_reason(&self) -> Option<String>;
    fn hidden_files_hint(&self) -> Option<String>;
    /// Shows the native dialog starting in the ssh directory.
    fn pick_file(&self) -> Result<PickOutcome, DialogError>;
}
```

실제 구현(`SystemBackend`)은 작업 10에서 만든다. 이 작업에서는 trait과 테스트용 가짜 구현만 만든다.

## App

```rust
pub struct App<B: Backend> { /* private */ }

/// Why the event loop should stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    Quit,
    /// Leave the TUI and run ssh with these arguments.
    Connect(Vec<OsString>),
}

impl<B: Backend> App<B> {
    /// Loads the configuration. With no hosts the app starts on the new-connection form.
    pub fn new(backend: B) -> Self;
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent);
    /// Bracketed paste: inserted into the focused text input (form field or filter).
    pub fn handle_paste(&mut self, text: &str);
    pub fn exit(&self) -> Option<&Exit>;
    /// True once after an action that requires a full terminal redraw (native dialog closed).
    pub fn take_full_redraw(&mut self) -> bool;
    pub fn mode(&self) -> &Mode;
    pub fn status(&self) -> Option<&Status>;
    pub fn backend(&self) -> &B;
    // plus read-only accessors the renderer needs (see "렌더러가 읽을 정보")
}
```

`handle_key`는 `KeyEventKind::Press`와 `Repeat`만 처리하고 `Release`는 무시한다. `Ctrl+C`는 모든 화면에서 `Exit::Quit`이다. 상태 메시지(`Status`)는 다음 키 입력이 처리되기 시작할 때 지워지고, 그 키 처리 결과로 새 메시지가 설정될 수 있다.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel { Info, Warning, Error }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status { pub level: StatusLevel, pub text: String }

pub enum Mode {
    List,
    Form(FormState),
    KeyPicker(KeyPickerState),   // overlay above a form; keeps the form
    Confirm(ConfirmState),       // diff preview before a write
    Notice(NoticeState),         // message that must be acknowledged
}
```

## 화면 1: 호스트 목록 (`Mode::List`)

상태: 전체 호스트(`ConfigSet::hosts`), 필터 문자열, 필터 입력 중 여부, 필터 결과 안에서의 선택 위치, 별칭별 `effective` 결과 캐시.

- 필터: 이름에 대한 대소문자 무시 부분 일치. 필터가 바뀌면 선택은 첫 항목.
- 선택된 항목의 적용값은 `Backend::effective(alias)`로 얻어 캐시한다. 같은 별칭은 다시 불러오기 전까지 한 번만 조회한다. 실패하면 오류 문자열을 캐시한다.
- 읽기 전용: `Backend::is_writable(entry.source)`가 false인 항목. 파일별로 한 번만 조회해 캐시한다.

키(필터 입력 중이 아닐 때):

| 키 | 동작 |
|---|---|
| `Up`, `k` / `Down`, `j` | 선택 이동(끝에서 멈춤) |
| `Home`, `g` / `End`, `G` | 처음, 끝 |
| `Enter` | 선택 항목 접속: `connect_args(paths, &config_prefix(), &ConnectTarget::Alias(name))` 결과로 `Exit::Connect`. 실패 시 Error 상태 메시지. 항목이 없으면 무시 |
| `n` | 새 접속 폼 |
| `e` | 선택 항목 편집 폼. 읽기 전용이면 폼을 열지 않고 Warning `"<file> is read-only"` |
| `d` | 선택 항목 삭제: `plan_delete` 후 `Mode::Confirm`. 읽기 전용이면 Warning. 계획 실패는 Error |
| `/` | 필터 입력 시작 |
| `r` | 다시 불러오기(캐시 비움, Info `"Reloaded"`) |
| `Esc` | 필터가 있으면 필터 해제, 없으면 무시 |
| `q` | `Exit::Quit` |

키(필터 입력 중): 문자 입력은 필터에 추가, `Backspace` 삭제, `Enter` 입력 종료(필터 유지), `Esc` 필터 해제와 입력 종료, `Up`/`Down` 선택 이동. 이 상태에서 `q`, `n`, `e`, `d`, `j`, `k`는 글자다.

불러오기:

- `load_config` 실패: 호스트 0개로 두고 Error 상태 메시지.
- `ConfigSet::warnings`가 있으면 Warning 상태 메시지(첫 경고의 내용과 전체 건수).
- 시작 시 호스트가 0개이면 `Mode::Form`(새 접속)으로 시작한다.

## 화면 2: 접속 폼 (`Mode::Form`)

필드 순서: Nickname, User, Address, Port, KeyFile. 새 접속과 편집 공용이다.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField { Nickname, User, Address, Port, KeyFile }

pub enum FormKind {
    New,
    Edit { entry: HostEntry /* plus the BlockFields read from the block */ },
}
```

텍스트 입력(`TextInput`): 값과 커서(문자 단위). 동작: 문자 삽입, `Backspace`, `Delete`, `Left`, `Right`, `Home`, `End`, `Ctrl+U`(전체 삭제). 제어 문자는 삽입하지 않는다. `handle_paste`는 첫 줄바꿈 전까지의 텍스트에서 제어 문자를 뺀 것을 커서 위치에 삽입한다. 커서의 화면 열 위치 계산(`unicode-width` 사용)을 제공한다.

필드 상태(편집 폼):

- 블록의 `FieldState::Value(v)`: 값 `v`로 채운다.
- `FieldState::Absent`: 빈 값. placeholder로 `effective`(해당 별칭의 `ssh -G` 결과)의 값을 제공한다(User는 `user`, Address는 `host_name`, Port는 `port`, KeyFile은 `identity_files`의 첫 항목). `effective` 조회가 실패하면 placeholder 없음.
- `FieldState::Unrepresentable { lines }`: 읽기 전용. 입력과 붙여넣기를 받지 않고, 키 목록 선택과 찾아보기로도 바뀌지 않는다. 원문 줄(`lines`)을 표시용으로 제공한다. 저장 시 항상 `Change::Keep`.
- Nickname은 `entry.name`으로 채운다.
- `entry.siblings`가 비어 있지 않으면 공유 알림을 제공한다: 옵션 변경은 같은 줄의 다른 패턴(`siblings` 나열)에도 적용되고 이름 변경은 이 별칭에만 적용된다는 내용.

새 접속 폼은 모든 필드가 비어 있고 placeholder가 없다.

키:

| 키 | 동작 |
|---|---|
| `Tab`, `Down` / `Shift+Tab`(`BackTab`), `Up` | 다음, 이전 필드(순환) |
| 문자와 편집 키 | 포커스 필드의 `TextInput` |
| `Ctrl+L` 또는 `F2` | 키 목록 열기: `Backend::discover_keys()`. 비어 있으면 Warning `"No private keys found in <ssh_dir>"`, 있으면 `Mode::KeyPicker` |
| `Ctrl+O` 또는 `F3` | 찾아보기: `picker_unavailable_reason()`이 있으면 그 사유를 Warning으로 표시하고 다이얼로그를 띄우지 않는다. 없으면 `pick_file()` 호출 후 full redraw 요청. `Picked(path)`이면 KeyFile 값을 `paths.abbreviate_home(&path)`로 설정, `Cancelled`이면 변화 없음(기존 입력값 유지), 오류면 Error |
| `Enter` | 제출(아래) |
| `Esc` | 목록으로 복귀. 호스트가 0개이면 `Exit::Quit` |

제출 전 검증(오류가 있으면 제출하지 않고 첫 오류 필드로 포커스를 옮기고 Error 상태 메시지):

- User: `validate_user`, Address: `validate_address`, Port: `parse_port`, KeyFile: `validate_key_file`. 읽기 전용 필드는 검증하지 않는다.
- 새 접속: Nickname이 비어 있지 않으면 `validate_nickname`과 전체 목록에 대한 `check_nickname_unique`. Address는 필수.
- 편집: Nickname은 필수(`validate_nickname`), 이름이 바뀌었으면 자기 자신을 제외한 중복 검사. Address가 비어 있는 것은 블록의 HostName이 원래 `Absent`일 때만 허용하고, 원래 값이 있었는데 비우면 `Empty { field: Address }` 오류.

필드별 실시간 검증 결과(현재 입력에 대한 `Option<ValidationError>`)를 렌더러용으로 제공한다.

미리보기(실시간, 렌더러용 문자열 `Result<String, String>`):

- 새 접속, Nickname 비어 있음: `preview(connect_args(.., Direct{..}))`. 예: `ssh -p 2222 -i /home/u/.ssh/id_ed25519 alice@example.com`
- 새 접속, Nickname 있음 또는 편집: `preview(connect_args(.., Alias(nickname)))`. 예: `ssh web`
- 검증 실패 시 `Err(오류 메시지)`.

제출 동작:

1. 새 접속, Nickname 비어 있음: 저장 없이 `Exit::Connect(connect_args(.., Direct{..}))`.
2. 새 접속, Nickname 있음: `plan_add`(비어 있는 필드는 `None`) 후 확인 화면 없이 바로 `commit`.
   - 성공: 다시 불러오고, `effective(nickname)`과 `find_mismatches`로 입력값을 대조한다. 불일치가 없으면 `Exit::Connect(Alias)`. 불일치가 있으면 `Mode::Notice`에 불일치 목록(`Mismatch`의 `Display`)을 보여주고 `Enter`는 접속, `Esc`는 목록 복귀. `effective` 조회 실패는 불일치 없음으로 보고 접속한다.
   - `WriteError::Stale`: 다시 불러오고 폼을 유지한 채 Error `"<path> was changed by another program; reloaded, press Enter to retry"`.
   - 그 외 오류: 폼 유지, Error.
3. 편집: `Change::from_form`으로 `OptionChanges`를 만들고 `plan_edit`. `is_noop()`이면 목록으로 복귀하며 Info `"No changes"`. 아니면 `Mode::Confirm`.

## 화면 3: 변경 확인 (`Mode::Confirm`)

상태: `PendingWrite`(diff hunk 포함), 작업 종류(편집 저장 또는 삭제), 대상 별칭, 스크롤 위치, 그리고 편집의 경우 돌아갈 폼 상태.

| 키 | 동작 |
|---|---|
| `y`, `Enter` | `commit` |
| `n`, `Esc` | 취소: 편집은 폼으로 복귀(입력값 유지), 삭제는 목록으로 복귀 |
| `Up`/`Down`, `PageUp`/`PageDown` | 스크롤 |

`commit` 결과:

- 성공: 다시 불러오고 목록으로 복귀. Info `"Saved <alias> (backup: <backup path>)"` 또는 `"Deleted <alias> (backup: <backup path>)"`. 편집 저장의 경우 저장한 값(읽기 전용 필드 제외, 비어 있는 필드 제외)을 `effective(새 별칭)`과 `find_mismatches`로 대조해 불일치가 있으면 `Mode::Notice`(`Enter`와 `Esc` 모두 목록 복귀).
- `Stale`: 다시 불러오고 목록으로 복귀, Error 메시지.
- 그 외 오류: 목록으로 복귀, Error 메시지.
- 저장이나 삭제 후 목록의 선택은 같은 이름의 항목이 있으면 그 항목, 없으면 범위 안으로 보정한다.

## 화면 4: 키 목록 (`Mode::KeyPicker`)

`Up`/`Down`(및 `k`/`j`) 이동, `Enter` 선택(KeyFile 값을 `paths.abbreviate_home(&key.path)`로 설정하고 폼으로 복귀, 포커스는 KeyFile), `Esc` 취소(폼 유지).

## 화면 5: 알림 (`Mode::Notice`)

제목과 여러 줄의 본문, `Enter` 동작(접속 또는 목록 복귀), `Esc`는 목록 복귀.

## 렌더러가 읽을 정보

다음을 읽기 전용 접근자나 pub 필드로 제공한다(이름은 자유).

- 목록: 필터 결과 항목들(이름, 출처 파일, 줄 번호, 읽기 전용 여부), 선택 위치, 필터 문자열과 입력 중 여부, 선택 항목의 적용값(`Result<EffectiveConfig, String>`), 전체 호스트 수
- 폼: 종류(새 접속 또는 편집), 필드별 값과 커서 열, placeholder, 읽기 전용 여부와 원문 줄, 실시간 검증 오류, 포커스, 미리보기, 공유 알림, 찾아보기 비활성 사유(`picker_unavailable_reason`), 숨김파일 안내(`hidden_files_hint`)
- 확인: 제목, 대상 파일 경로, hunk 목록, 스크롤 위치
- 키 목록: `KeyInfo` 목록과 선택 위치
- 알림: 제목과 본문 줄
- 상태 메시지

## 테스트

가짜 `Backend`(테스트 모듈 안): 메모리의 `ConfigSet`을 돌려주고, `commit` 호출을 기록하며 미리 정한 결과를 반환하고, `effective` 호출 횟수를 세고, `pick_file` 결과를 지정할 수 있어야 한다. `ConfigSet`은 `ParsedConfig::parse`와 `HostEntry`를 직접 구성하거나, `tempfile` 임시 디렉터리에 config를 쓰고 `config::load`로 만든다. 가짜 `commit`이 실제로 임시 디렉터리에 쓰도록(`PendingWrite::commit` 위임) 구성하는 통합형 테스트도 포함한다. 실제 `~/.ssh`, 실제 ssh 실행, 실제 다이얼로그는 사용하지 않는다.

목록:

1. 호스트가 없으면 폼으로 시작하고 `Esc`는 `Exit::Quit`
2. 호스트가 있으면 목록으로 시작, 이동 키와 경계 처리
3. `Enter`는 `Exit::Connect(["web"])`, prefix가 있으면 맨 앞에 포함
4. 필터: `/` 입력 후 글자 입력으로 결과가 줄고, 필터 입력 중 `q`는 종료가 아니라 글자, `Enter`로 입력 종료 후 `q`는 종료, `Esc`로 해제
5. 적용값 캐시: 같은 항목을 여러 번 선택해도 `effective` 호출 1회, `r` 이후 다시 호출
6. 읽기 전용 항목에서 `e`와 `d`는 Warning이고 모드가 바뀌지 않음, `Enter` 접속은 가능
7. `q`와 `Ctrl+C` 종료, `Release` 이벤트 무시
8. 불러오기 실패 시 Error 상태, `ConfigSet::warnings`가 있으면 Warning 상태
9. 상태 메시지는 다음 키 입력에서 지워짐

폼(새 접속):

10. 필드 이동 순환(`Tab`, `BackTab`)
11. Nickname 없이 제출: `Exit::Connect(["-p", "2222", "-i", "<home>/.ssh/id", "alice@example.com"])`, `commit` 호출 없음
12. Address 없이 제출: 종료하지 않고 Error, 포커스가 Address
13. `-`로 시작하는 주소와 유저, 잘못된 포트는 제출 거부
14. Nickname 있음: `commit` 1회 호출되고 `PendingWrite`의 내용에 `Host <nickname>` 블록 포함, 불일치 없으면 `Exit::Connect([nickname])`
15. 중복 Nickname 거부(`commit` 호출 없음)
16. 저장 후 불일치(가짜 `effective`가 다른 User 반환): `Mode::Notice`에 불일치 문구, `Enter`는 `Exit::Connect`, `Esc`는 목록
17. `commit`이 `Stale`을 반환하면 폼 유지와 Error, 입력값 유지
18. 미리보기: 빈 Nickname일 때 직접 접속 명령, Nickname 입력 후 `ssh <nickname>`, 검증 실패 시 `Err`
19. 실시간 검증 오류가 필드별로 제공됨

폼(편집):

20. 값 채움: `Value`는 값, `Absent`는 빈 값과 placeholder, `Unrepresentable`은 읽기 전용이며 입력해도 변하지 않음
21. 변경 없이 제출하면 목록 복귀와 Info `"No changes"`, `commit` 호출 없음
22. User 변경 후 제출하면 `Mode::Confirm`, hunk에 `Removed`와 `Added` 줄
23. 확인에서 `n`은 폼 복귀(입력값 유지), `y`는 `commit` 후 목록 복귀와 Info
24. 필드를 비우면 해당 줄 삭제 계획(`Removed`만 있는 hunk)
25. 원래 HostName이 있는 블록에서 Address를 비우면 제출 거부, 원래 `Absent`이면 허용
26. 이름 변경: 다른 호스트와 중복이면 거부, 유효하면 확인 화면을 거쳐 저장
27. 여러 패턴 블록의 항목 편집 시 공유 알림 제공
28. 저장 후 불일치가 있으면 `Mode::Notice`, 닫으면 목록
29. `commit`이 `Stale`이면 목록 복귀와 Error, 다시 불러오기 수행(`load_config` 호출 횟수로 확인)

삭제:

30. `d` 후 `Mode::Confirm`, `Esc`는 목록 복귀와 `commit` 호출 없음, `y`는 `commit` 후 목록에서 항목 제거(통합형: 임시 디렉터리의 실제 파일로 확인)

키 목록과 찾아보기:

31. `Ctrl+L`로 `Mode::KeyPicker`, `Enter`로 선택하면 KeyFile이 `~/.ssh/<name>` 형태로 설정되고 폼 복귀, `Esc`는 값 유지
32. 키가 없으면 Warning이고 모드 유지
33. 찾아보기 비활성 사유가 있으면 `pick_file` 호출 없이 Warning
34. `Picked`이면 값 설정과 `take_full_redraw()`가 true 한 번, `Cancelled`이면 기존 입력값 유지와 full redraw 요청, 오류면 Error
35. KeyFile이 읽기 전용이면 키 목록 선택과 찾아보기로 값이 바뀌지 않음

입력:

36. `TextInput`: 삽입, 삭제, 커서 이동, 한글과 전각 문자의 커서 열 계산, 제어 문자 무시
37. `handle_paste`: 줄바꿈 이후는 버리고 제어 문자 제거, 읽기 전용 필드에는 무시, 필터 입력 중에는 필터에 삽입

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- 위 테스트가 모두 존재하고 기존 테스트 210개 유지
- `src/app/` 안에 `std::fs`, `std::process`, `std::env`, 터미널 출력 사용 없음(테스트 모듈 제외)
