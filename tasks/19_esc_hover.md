# 작업 19: esc로 종료, 마우스 호버 애니메이션

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `src/app/mod.rs`(키 처리), `src/ui/`(렌더링, `HitMap`, `Theme`), `src/tui.rs`(이벤트 루프)를 읽는다.

## 목표

1. 호스트 목록에서 `esc`로 프로그램을 종료한다.
2. 마우스를 누를 수 있는 요소 위에 올리면 그 요소가 부드럽게 밝아지고, 벗어나면 부드럽게 원래대로 돌아간다(호버 애니메이션).

## 대상 파일

- `src/app/mod.rs`, `src/app/tests.rs`
- `src/ui/`(신규 `src/ui/hover.rs` 포함)
- `src/tui.rs`

## 1. esc로 종료

- 목록 화면(필터 입력 중이 아닐 때)의 `Esc`: 필터가 있으면 필터 해제(기존과 같음), 필터가 없으면 `Exit::Quit`.
- 필터 입력 중의 `Esc`는 기존과 같다(필터 해제와 입력 종료). 그 뒤 한 번 더 누르면 종료된다.
- 폼, 변경 확인, 팝업의 `Esc`는 기존과 같다(뒤로, 취소, 닫기).
- `q`와 `Ctrl+C`는 계속 종료로 동작한다.
- 목록의 키 안내에서 마지막 쌍을 `q quit` 대신 `esc quit`으로 바꾼다(클릭 대상은 `Key(Esc)`). 필터가 적용된 상태(입력 중은 아님)에서는 `esc clear filter`로 표시한다.
- `src/app/tests.rs`의 기존 테스트 가운데 "필터가 없을 때 목록의 `Esc`는 아무 일도 하지 않는다"를 가정한 단언만 새 동작에 맞게 고친다. 고친 단언은 최종 보고에 적는다. 그 외 단언은 바꾸지 않는다.

추가 테스트(app):

1. 필터 없는 목록에서 `Esc`는 `Exit::Quit`
2. 필터가 적용된 목록에서 첫 `Esc`는 필터 해제(종료 아님), 둘째 `Esc`는 종료
3. 필터 입력 중 `Esc`는 필터 해제와 입력 종료이고 종료 아님
4. 폼에서 `Esc`는 목록 복귀이고 종료 아님(호스트가 있을 때), 팝업에서 `Esc`는 닫기
5. `q`는 여전히 종료
6. `ClickTarget::Key(KeyCode::Esc)` 클릭도 같은 규칙

추가 테스트(ui): 목록 키 안내의 마지막 쌍이 `esc quit`이고 `q quit`이 없음, 필터 적용 상태에서는 `esc clear filter`, 그 쌍의 클릭 대상이 `Key(Esc)`.

## 2. 호버 애니메이션

### 호버 대상

`HitMap`에 등록된 영역 가운데 `ClickTarget::Inert`와 `ClickTarget::Outside`를 뺀 모든 요소가 호버 대상이다: 호스트 줄, 필터 바, 폼 필드 줄, 키 안내 쌍, 키 목록 줄, 머리줄의 업데이트 알림.

같은 요소인지의 판정은 "호버 키"로 한다. `FormField { field, column }`은 `column`을 무시하고 `field`만 비교하고, 나머지는 `ClickTarget` 값 그대로 비교한다. `ClickTarget`에 `pub fn hover_key(&self) -> Option<HoverKey>`(또는 같은 역할의 함수)를 두고 `Inert`와 `Outside`는 `None`을 돌려준다.

### 상태와 시간 (src/ui/hover.rs, 순수)

```rust
/// Tracks which element the pointer is over and the fades in progress. Pure; the caller supplies time.
#[derive(Debug, Default)]
pub struct Hover { /* current element with its start time, elements that are fading out */ }

impl Hover {
    /// Fade duration in each direction.
    pub const DURATION: Duration = Duration::from_millis(150);

    /// Sets the element under the pointer (None when over nothing hoverable).
    /// Returns true when the hovered element changed.
    pub fn set(&mut self, key: Option<HoverKey>, now: Instant) -> bool;

    /// Hover strength of an element at `now`, from 0.0 (not hovered) to 1.0 (fully hovered).
    pub fn intensity(&self, key: &HoverKey, now: Instant) -> f32;

    /// True while any fade is still in progress at `now`.
    pub fn is_animating(&self, now: Instant) -> bool;

    /// Forgets everything (used when the pointer is known to be gone).
    pub fn clear(&mut self);
}
```

규칙:

- 요소에 들어가면 강도가 0에서 1로 `DURATION` 동안 올라간다. 진행률 `p`에 ease-out(`1 - (1 - p)^3`)을 적용한다.
- 요소에서 벗어나면 그 순간의 강도에서 0까지 `DURATION`에 비례한 시간 동안 내려간다(강도 0.5에서 벗어나면 75ms). 내려가는 요소는 여러 개일 수 있다(마우스를 줄 위로 빠르게 훑으면 지나온 줄들이 차례로 어두워진다). 끝난 항목은 버리고 동시에 추적하는 수는 최대 16개로 제한한다(넘치면 가장 오래된 것부터 버림).
- 내려가는 중인 요소에 다시 들어가면 그 순간의 강도에서 이어서 올라간다(강도가 튀지 않는다).
- 같은 요소로 `set`을 다시 부르면 아무 것도 바뀌지 않고 false를 돌려준다.
- `intensity`와 `is_animating`은 상태를 바꾸지 않는다. `now`가 시작 시각보다 이르면 0으로 취급한다.

### 렌더링

`ui::render`가 호버 상태와 현재 시각을 받는다.

```rust
pub fn render<B: Backend>(
    frame: &mut Frame,
    app: &App<B>,
    theme: &Theme,
    hover: Option<(&Hover, Instant)>,
) -> HitMap;
```

`hover`가 `None`이면 지금과 똑같이 그린다. 강도 `t`(0.0-1.0)에 따른 표현(TrueColor 모드, 색은 두 RGB 값의 선형 보간):

| 요소 | 표현 |
|---|---|
| 호스트 줄(선택 아님) | 줄 배경 BG → `#1e2128`(HOVER_ROW), 요약 글자 MUTED → TEXT |
| 호스트 줄(선택됨) | 줄 배경 ROW → `#2e343f`(HOVER_SELECTED) |
| 필터 바(입력 중 아님) | 배경 INPUT → INPUT_FOCUS |
| 폼 필드 줄(포커스 아님, 읽기 전용 아님) | 입력칸 배경 INPUT → INPUT_FOCUS, 라벨 MUTED → TEXT |
| 폼 필드 줄(읽기 전용) | 라벨 MUTED → TEXT |
| 키 안내 쌍(배지 아님) | 쌍 전체 배경 BG → ROW, 설명 글자 MUTED → TEXT |
| 키 안내 주 동작(배지) | 배지 배경 ACCENT → `#9be5d6`(HOVER_ACCENT). 비활성 배지는 변화 없음 |
| 키 목록 줄(선택 아님) | 줄 배경 BG → HOVER_ROW |
| 키 목록 줄(선택됨) | 줄 배경 ROW → HOVER_SELECTED |
| 업데이트 알림 | 배경 BG → ROW |

- 포커스된 폼 필드, 입력 중인 필터 바는 이미 강조 상태이므로 호버 표현을 더하지 않는다.
- 팝업 뒤의 흐려진 화면은 호버 표현이 없다(그 영역은 `Outside`다).
- 색 보간 함수는 `theme.rs`에 둔다(`fn mix(from: Color, to: Color, t: f32) -> Color`. 두 색이 모두 `Rgb`일 때만 보간하고, 아니면 `t >= 0.5`에서 `to`). 새 색 3개(HOVER_ROW, HOVER_SELECTED, HOVER_ACCENT)도 `theme.rs`에 정의한다.
- Ansi 모드는 중간 색이 없으므로 애니메이션 없이 `t >= 0.5`일 때 요소의 주 글자(호스트 이름, 필드 라벨, 키 안내의 키, 키 목록의 경로, 업데이트 알림 문구)에 밑줄을 준다.
- `t`가 0인 요소는 호버 인자가 없을 때와 셀 단위로 똑같이 그려져야 한다.

### 이벤트 루프 (src/tui.rs)

- 마우스가 켜져 있을 때만 호버를 쓴다(`options.mouse`가 false이면 `render`에 `None`을 넘기고 이동 이벤트를 무시).
- 루프는 마지막 포인터 위치(`Option<(u16, u16)>`)와 `Hover`를 보관한다.
- `MouseEventKind::Moved`와 `Drag(_)`: 포인터 위치를 갱신하고 `hits.hit(col, row)`의 호버 키로 `hover.set`을 부른다. 바뀌었으면 다시 그리고, 바뀌지 않았으면 다시 그리지 않는다.
- 다른 이벤트로 화면이 바뀐 뒤에는 포인터가 움직이지 않아도 그 아래 요소가 달라질 수 있다. 매번 그린 직후 마지막 포인터 위치로 새 `HitMap`에서 호버 키를 다시 구해 `hover.set`을 부르고, 바뀌었으면 한 번 더 그린다.
- 애니메이션 진행: `hover.is_animating(now)`인 동안에는 `event::read()`로 막히지 말고 `event::poll(Duration::from_millis(16))`로 기다린다. 이벤트가 없으면 다음 프레임을 그린다. 애니메이션이 끝나면 다시 블로킹 `event::read()`로 돌아가 유휴 상태에서 CPU를 쓰지 않는다.
- 클릭, 휠, 키, 붙여넣기 처리와 종료 처리(남은 입력 비우기)는 기존과 같다.
- 루프의 판단 부분은 테스트할 수 있게 순수 함수나 작은 구조체로 분리한다(예: 이벤트 하나를 처리하고 "다시 그릴지"를 돌려주는 기존 `handle_event`에 포인터 위치와 `Hover`를 포함한 컨텍스트를 넘기는 방식).

### 테스트

hover.rs:

7. 들어간 직후 강도 0, `DURATION/2`에서 0보다 크고 1보다 작으며 선형(0.5)보다 큼(ease-out), `DURATION` 이후 1.0
8. 벗어난 뒤 강도가 내려가 `DURATION` 이후 0, `is_animating`이 false
9. 강도 0.5 부근에서 벗어나면 `DURATION`보다 짧은 시간에 0
10. 내려가는 중 같은 요소에 다시 들어가면 그 시점 강도가 연속(차이 0.05 이하)이고 이후 1.0에 도달
11. 요소 A에서 B로 옮기면 A는 내려가고 B는 올라가며 둘 다 `intensity`로 조회 가능
12. 같은 키로 `set`을 반복하면 false, 다른 키면 true, `None`으로 바꾸면 true
13. 17개 요소를 빠르게 지나가도 추적 수가 16을 넘지 않고 패닉 없음
14. `FormField`의 호버 키는 `column`이 달라도 같음, `Inert`와 `Outside`는 키 없음
15. `mix`: `t` 0과 1의 끝값, 0.5의 중간값(채널별 반올림), `Rgb`가 아닌 색의 계단 동작

ui(`TestBackend`, TrueColor):

16. 호스트 줄: 강도 0이면 호버 인자가 없을 때와 버퍼가 완전히 같음, 강도 1이면 그 줄 배경이 HOVER_ROW이고 요약이 TEXT, 중간 시각이면 배경이 BG와 HOVER_ROW 사이의 값이고 다른 줄은 변화 없음
17. 선택된 호스트 줄의 호버 배경 HOVER_SELECTED
18. 필터 바, 폼 필드(포커스 아님), 키 안내 쌍, 배지, 키 목록 줄, 업데이트 알림 각각의 강도 1 표현
19. 포커스된 폼 필드와 입력 중인 필터 바는 강도 1이어도 변화 없음
20. 비활성 배지는 변화 없음
21. Ansi 모드: 강도 1에서 호스트 이름에 밑줄이 있고 배경은 그대로, 강도 0.4에서는 밑줄 없음
22. 팝업이 떠 있을 때 뒤 화면의 호스트 줄 키로 강도 1을 주어도 뒤 화면은 변화 없음
23. 작은 화면(1x1, 5x2, 20x5)에서 호버 인자가 있어도 패닉 없음

tui:

24. `Moved`로 호스트 줄 위에 오면 다시 그리기 요청이 true이고 `Hover`의 현재 키가 그 줄, 같은 줄 안에서 다시 `Moved`하면 false
25. 줄에서 빈 곳(`None`)으로 `Moved`하면 true이고 현재 키 없음
26. `options.mouse`가 false인 컨텍스트에서는 `Moved`가 무시됨
27. 키 입력으로 화면이 바뀐 뒤(예: 목록에서 `n`으로 폼 열기) 같은 포인터 위치의 호버 키가 새 `HitMap` 기준으로 다시 계산됨(이 재계산을 담당하는 함수 단위로 테스트)

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- `cargo check --offline --target x86_64-pc-windows-msvc --all-targets`와 `cargo check --offline --target x86_64-unknown-linux-gnu --all-targets` 통과
- 위 테스트가 모두 존재하고 전체 테스트 수가 369개 이상
- `docs/design/` 변경 없음
