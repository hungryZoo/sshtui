# 작업 15 재지시 1: 종료 뒤 남는 마우스 입력

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 작업 15(`tasks/15_mouse.md`)의 결과물에 대한 수정이다.

## 대상 파일

- `src/tui.rs`

## 문제

위치: `handle_event`의 `MouseEventKind::Down(MouseButton::Left)` 처리와 `run_loop`의 종료 처리.

클릭을 버튼 누름(Down) 시점에 처리하므로, 그 클릭으로 앱이 종료되면(호스트 줄 더블 클릭으로 접속, `q quit` 안내 클릭, `[ Connect ]` 버튼 등) 뒤따르는 버튼 뗌(Up) 이벤트가 처리되지 않은 채 터미널 입력에 남는다. 터미널 복원 뒤 이 바이트(`ESC [ < 0 ; x ; y m`)가 다음 프로세스의 입력으로 들어간다.

재현(100x30 tmux 창에서 SGR 마우스 시퀀스를 직접 입력):

- 호스트 줄에서 Down, Up, Down, Up을 연속으로 보내면 ssh가 실행된 뒤 셸에 `^[[<0;5;3m`이 그대로 찍힌다.
- 같은 바이트가 ssh의 암호 프롬프트 입력으로 들어가면 잘못된 암호가 한 번 전송된다.

## 기대 동작

1. 클릭은 버튼 뗌 시점에 처리한다.
   - `Down(Left)`: 그 칸의 대상(`HitMap::hit`)을 "눌린 대상"으로 기억만 하고 앱 상태는 바꾸지 않는다(다시 그리지 않음).
   - `Up(Left)`: 뗀 칸의 대상이 눌린 대상과 같을 때만 클릭으로 처리한다(`ClickTracker::register`로 더블 클릭 판정 후 `app.handle_click`). 다르면 아무 일도 없다. 어느 경우든 눌린 대상 기억은 지운다.
   - `ClickTracker`의 판정 규칙(같은 대상, 같은 칸, 400ms 이내)은 그대로이고 시각은 Up 시점이다.
   - 눌린 대상의 기억은 `ClickTracker` 안에 두어도 되고 별도 상태로 두어도 된다. 순수하게 테스트할 수 있어야 한다.
2. 종료 시 남은 입력을 버린다.
   - 이벤트 루프를 벗어난 뒤, `options.mouse`가 true이면 `DisableMouseCapture`를 먼저 실행하고 출력을 flush한 다음, raw mode가 아직 켜진 상태에서 대기 중인 이벤트를 읽어 버린다: `event::poll(Duration::from_millis(50))`이 true인 동안 `event::read()`를 반복하고, 전체 대기는 최대 250ms로 제한한다.
   - 그 뒤 나머지 복원 단계를 수행한다(기존 `restore_terminal`은 그대로 호출해도 된다. `DisableMouseCapture`가 두 번 실행되어도 무방하다).
   - `options.mouse`가 false이면 이 대기를 하지 않는다(종료 지연 없음).
   - 오류나 패닉 경로의 복원(Drop 가드, 패닉 훅)에는 대기를 넣지 않는다.

## 추가할 테스트

1. `handle_event`: `Down(Left)`만으로는 앱 상태가 바뀌지 않고 다시 그리기 요청(반환값)도 없음
2. `handle_event`: 같은 호스트 줄에서 `Down` 다음 `Up`이면 선택이 바뀜
3. `handle_event`: `Down`한 칸과 다른 대상 위에서 `Up`하면 아무 일도 없음(버튼 위에서 누르고 밖에서 뗌)
4. `handle_event`: 누름 없이 `Up`만 오면 아무 일도 없음
5. 더블 클릭: `Down, Up, Down, Up`(400ms 이내, 같은 칸)에서 두 번째 `Up`에 `Exit::Connect`가 설정되고 첫 번째 `Up`에는 설정되지 않음. 시간 주입이 필요하면 `handle_event`가 시각을 인자로 받도록 바꾼다.
6. 기존 `handle_event` 테스트 가운데 Down 시점 처리를 가정한 것은 Up 시점 처리에 맞게 고친다. 고친 테스트는 최종 보고에 적는다.

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- `cargo check --offline --target x86_64-pc-windows-msvc --all-targets`와 `cargo check --offline --target x86_64-unknown-linux-gnu --all-targets` 통과
- 기존 테스트 357개 유지, 위 테스트 추가
