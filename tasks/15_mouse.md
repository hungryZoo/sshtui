# 작업 15: 마우스 지원

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `src/ui/`(특히 `buttons.rs`, `layout.rs`, 키 안내 `KeyHint`), `src/app/`, `src/tui.rs`, `src/main.rs`를 읽는다.

## 목표

화면의 줄, 필드, 버튼, 키 안내를 마우스로 누를 수 있게 하고 휠 스크롤을 지원한다. 키보드 동작은 그대로 둔다.

구조는 3단계다.

1. 렌더러가 그리면서 클릭 영역 목록(`HitMap`)을 만든다.
2. 이벤트 루프가 마우스 좌표를 `HitMap`에서 `ClickTarget`으로 바꾸고 더블 클릭을 판정한다.
3. `App`이 `ClickTarget`을 받아 상태를 바꾼다(순수 상태 전이, 가짜 `Backend`로 테스트).

## 대상 파일

- `src/app/`(클릭 대상 타입, 처리, 테스트 추가)
- `src/ui/`(`HitMap` 생성, 테스트 추가)
- `src/tui.rs`, `src/main.rs`

기존 테스트의 단언은 바꾸지 않는다. `ui::render`의 반환형이 바뀌어 호출부가 달라지는 것은 허용한다.

## App (src/app)

```rust
/// Something on screen that can be clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    /// Row `index` of the filtered host list.
    HostRow(usize),
    /// A form field; `column` is the display column inside the whole value (0 = first cell of the
    /// value, already adjusted for horizontal scrolling).
    FormField { field: FormField, column: u16 },
    /// Row `index` of the key picker.
    KeyRow(usize),
    /// A button or key hint that behaves exactly like pressing this key without modifiers.
    Key(KeyCode),
    /// The update notice in the header.
    UpdateNotice,
    /// Inside a popup or panel but not on a control: the click is swallowed.
    Inert,
    /// Outside the active popup.
    Outside,
}

impl<B: Backend> App<B> {
    /// Handles a left click. `double` is true for the second click of a double click on the same target.
    pub fn handle_click(&mut self, target: ClickTarget, double: bool);
    /// Handles one wheel step. `down` is true when scrolling down.
    pub fn handle_scroll(&mut self, down: bool);
}
```

공통: `exit`가 이미 정해졌으면 무시한다. 처리된 클릭은 키 입력과 같이 상태 메시지를 먼저 지운다. 현재 화면에서 의미 없는 대상은 무시한다(상태 메시지도 지우지 않음). `Inert`는 항상 무시한다.

| 화면 | 대상 | 동작 |
|---|---|---|
| 모든 화면 | `Key(code)` | `handle_key(KeyEvent::new(code, KeyModifiers::NONE))`와 동일 |
| 목록 | `HostRow(i)` | `i`번 줄 선택(범위 밖이면 무시)과 적용값 갱신. `double`이면 이어서 접속(`Enter`와 동일). 필터 입력 중이면 입력 상태는 유지 |
| 목록 | `UpdateNotice` | `u`와 동일(계획이 없으면 무시) |
| 목록 | 휠 | 선택을 한 줄 위 또는 아래로(끝에서 멈춤) |
| 폼 | `FormField { field, column }` | 그 필드로 포커스 이동. 읽기 전용이 아니면 커서를 `column`에 해당하는 문자 위치로 이동 |
| 폼 | 휠 | 무시 |
| 확인 | 휠 | 한 줄 스크롤(`Up`/`Down`과 동일) |
| 키 목록 | `KeyRow(i)` | `i`번 줄을 선택하고 바로 확정(`Enter`와 동일). 범위 밖이면 무시 |
| 키 목록 | `Outside` | `Esc`와 동일 |
| 키 목록 | 휠 | 선택 이동 |
| 알림, Update | `Outside` | `Esc`와 동일 |

`TextInput`에 `pub(super) fn set_cursor_to_column(&mut self, column: usize)`를 추가한다. 표시 폭 기준으로, 각 문자의 시작 열이 `column` 이하인 마지막 문자 뒤가 아니라 "그 열에 있는 문자의 앞"으로 커서를 둔다. 값의 표시 폭 이상이면 맨 끝이다. 전각 문자(폭 2)의 둘째 칸을 누르면 그 문자의 앞이다.

## 렌더러 (src/ui)

```rust
/// Clickable regions of the last rendered frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HitMap { /* private: Vec<(Rect, ClickTarget)> */ }
impl HitMap {
    /// Target at a terminal cell; regions registered later win over earlier ones.
    pub fn hit(&self, column: u16, row: u16) -> Option<&ClickTarget>;
}

/// Draws the screen and returns its clickable regions. No I/O.
pub fn render<B: Backend>(frame: &mut Frame, app: &App<B>) -> HitMap;
```

등록할 영역:

- 맨 아래 키 안내: `<키> <설명>` 쌍마다 그 쌍이 차지한 칸 전체를 `Key(code)`로 등록한다. 대응: `Enter`는 `KeyCode::Enter`, `Esc`는 `KeyCode::Esc`, 한 글자 키(`n`, `e`, `d`, `/`, `r`, `u`, `q`, `y`)는 `KeyCode::Char`. 복합 표기는 첫 키를 쓴다(`Tab/Shift+Tab move`는 `KeyCode::Tab`, `y/Enter confirm`은 `Char('y')`, `n/Esc cancel`은 `Char('n')`, `Enter/Esc back to list`는 `Enter`). `Up/Down scroll`은 등록하지 않는다. `KeyHint`에 대응 `KeyCode`를 함께 담아 문구와 동작이 한 곳에서 정의되게 한다.
- 목록 화면: 호스트 줄마다 패널 안쪽 폭 전체를 `HostRow(필터 결과 인덱스)`로 등록한다(스크롤된 경우 보이는 줄의 실제 인덱스). 머리줄의 업데이트 알림 칸은 `UpdateNotice`.
- 폼 화면: 필드마다 라벨과 입력 영역을 포함한 그 줄 전체를 `FormField`로 등록한다. `column`은 입력 영역 안에서 누른 위치에 가로 스크롤 오프셋(표시 폭 기준)을 더한 값이고, 라벨이나 안내 열을 누르면 값의 끝(`u16::MAX`)으로 둔다. 영역 하나에 대상 하나만 등록할 수 있으므로, `HitMap`의 항목이 "영역과, 영역 안 좌표에서 `ClickTarget`을 만드는 방법"을 담도록 설계하거나 입력 영역을 칸 단위로 등록한다. 방법은 자유이되 `hit(column, row)`의 결과가 위 정의와 같아야 한다.
- 버튼: `[ Keys F2 ]`는 `Key(F(2))`, `[ Browse F3 ]`는 `Key(F(3))`(비활성 버튼도 등록한다. 누르면 기존처럼 사유가 상태 메시지로 나온다), `[ Connect ]`와 `[ Save ]`는 `Key(Enter)`, `[ Cancel ]`은 `Key(Esc)`, `[ Confirm y ]`는 `Key(Char('y'))`, `[ Cancel n ]`은 `Key(Char('n'))`, `[ Connect anyway ]`, `[ OK ]`, `[ Update and exit ]`는 `Key(Enter)`, `[ Back ]`, `[ Close ]`는 `Key(Esc)`.
- 팝업(키 목록, 알림, Update): 먼저 화면 전체를 `Outside`로, 이어서 팝업 영역 전체를 `Inert`로, 그 위에 줄(`KeyRow`)과 버튼을 등록한다. 팝업이 떠 있을 때 뒤 화면의 영역(호스트 줄, 폼 필드 등)은 등록하지 않거나 `Outside`에 가려져야 한다. 단, 맨 아래 키 안내는 팝업 중에도 동작한다.
- 화면 밖으로 잘린 요소는 등록하지 않는다. 폭이나 높이가 0인 영역은 등록하지 않는다.

## 이벤트 루프 (src/tui.rs)

```rust
/// Terminal features to enable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options { pub mouse: bool }

pub fn run<B: Backend>(app: App<B>, options: Options) -> std::io::Result<Exit>;

/// Detects double clicks. Pure; the caller supplies the time.
#[derive(Debug, Default)]
pub struct ClickTracker { /* last target, cell and time */ }
impl ClickTracker {
    /// Records a click and reports whether it completes a double click: same target, same cell,
    /// at most 400 ms after the previous click. A double click resets the tracker, so a third
    /// click starts over.
    pub fn register(&mut self, target: &ClickTarget, column: u16, row: u16, now: Instant) -> bool;
}
```

- `options.mouse`가 true이면 초기화 때 `EnableMouseCapture`를 실행한다. 복원 때는 항상 `DisableMouseCapture`를 실행한다(Drop 가드와 패닉 훅 포함, 순서는 다른 복원 단계보다 앞).
- 루프는 마지막으로 그린 프레임의 `HitMap`을 보관한다.
- `Event::Mouse` 처리:
  - `MouseEventKind::Down(MouseButton::Left)`: `HitMap::hit`으로 대상을 찾고, 있으면 `ClickTracker::register`로 더블 클릭을 판정해 `app.handle_click`.
  - `ScrollUp` / `ScrollDown`: `app.handle_scroll(false / true)`.
  - 그 외(이동, 드래그, 버튼 뗌, 다른 버튼)는 무시하고 다시 그리지 않는다. 이동 이벤트마다 화면을 다시 그리면 안 된다.
- 기존 `handle_event` 테스트를 유지하고, 시그니처가 바뀌면 호출만 맞춘다.

## main

- 환경변수 `SSHTUI_NO_MOUSE`가 비어 있지 않고 `0`이 아니면 `Options { mouse: false }`. 마우스 캡처는 터미널의 기본 텍스트 선택을 막으므로 끌 수 있어야 한다.
- `--help`에 한 줄 추가: `SSHTUI_NO_MOUSE  Disable mouse support (keeps the terminal's own text selection)`.
- 판정은 순수 함수(`fn mouse_enabled(value: Option<&OsStr>) -> bool`)로 분리하고 테스트한다.

## 테스트

app(가짜 `Backend`):

1. 목록: `HostRow(2)` 클릭으로 선택 이동과 적용값 조회, 범위 밖 인덱스는 무시
2. 목록: `double`이 true인 `HostRow` 클릭은 `Exit::Connect([alias])`
3. 목록: 휠 아래와 위로 선택 이동, 끝에서 멈춤
4. 목록: `Key(Char('n'))`은 폼 열기, `Key(Char('q'))`는 종료, `UpdateNotice`는 계획이 있을 때만 `Mode::Update`
5. 폼: `FormField` 클릭으로 포커스 이동과 커서 위치(`column` 0, 중간, 값보다 큰 값, 한글 값의 둘째 칸)
6. 폼: 읽기 전용 필드는 포커스만 이동
7. 폼: `Key(F(2))`로 키 목록, `Key(Enter)`로 제출, `Key(Esc)`로 복귀
8. 확인: `Key(Char('y'))`와 `Key(Char('n'))`, 휠 스크롤
9. 키 목록: `KeyRow(i)`로 선택과 확정(KeyFile 값 설정), `Outside`는 취소, `Inert`는 무시
10. 알림과 Update: `Outside`는 닫기, `Key(Enter)` 동작
11. 현재 화면과 무관한 대상(폼에서 `HostRow`, 목록에서 `KeyRow` 등)은 무시되고 상태 메시지가 지워지지 않음
12. 종료가 정해진 뒤의 클릭 무시
13. `TextInput::set_cursor_to_column`

ui(`TestBackend`로 그린 뒤 `HitMap` 검사):

14. 목록: 각 호스트 줄의 가운데 칸이 `HostRow(i)`, 스크롤된 목록에서 보이는 첫 줄의 인덱스가 실제 인덱스
15. 목록: 키 안내의 `n new` 위는 `Key(Char('n'))`, `Enter connect` 위는 `Key(Enter)`, 쌍 사이 공백은 None
16. 목록: 업데이트 알림 칸은 `UpdateNotice`, 계획이 없으면 None
17. 폼: 각 필드 줄의 입력 영역은 `FormField`이고 `column`이 누른 칸과 일치, 라벨 칸은 `column == u16::MAX`
18. 폼: 긴 값이 가로 스크롤된 상태에서 입력 영역 첫 칸의 `column`이 스크롤 오프셋과 일치
19. 폼: 버튼 4종의 대상, 비활성 Browse 버튼도 `Key(F(3))`
20. 확인: 버튼 2개의 대상
21. 팝업: 팝업 밖은 `Outside`, 팝업 안 빈 곳은 `Inert`, 키 줄은 `KeyRow(i)`, 버튼은 `Key(...)`, 팝업 뒤의 호스트 줄 위치는 `Outside` 또는 `Inert`
22. 팝업 중에도 맨 아래 키 안내는 `Key(...)`
23. 작은 화면(1x1, 5x2, 20x5)에서 패닉 없음, 등록된 모든 영역이 화면 안에 있음

tui와 main:

24. `ClickTracker`: 같은 대상과 칸에서 400ms 이내 두 번째 클릭은 true, 401ms는 false, 다른 칸이나 다른 대상은 false, 세 번째 연속 클릭은 false
25. `mouse_enabled`: None, 빈 값, `0`은 true, `1`은 false

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- `cargo check --offline --target x86_64-pc-windows-msvc --all-targets`와 `cargo check --offline --target x86_64-unknown-linux-gnu --all-targets` 통과
- 위 테스트가 모두 존재하고 기존 테스트 330개 유지
