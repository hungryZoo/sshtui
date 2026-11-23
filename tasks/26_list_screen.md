# 작업 26: 목록 화면 개선 (마지막 접속 커서, new 줄, forget 표기, 삭제 기본값 cancel)

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 작업 24의 `src/state.rs`(`default_state_file`, `read_last_host`, `write_last_host`)를 사용한다.

## 목표

1. 시작할 때 마지막으로 접속한 호스트에 커서를 둔다(바로 `Enter`로 접속 가능).
2. 호스트 목록의 맨 아래에 새 접속 줄을 둔다.
3. 키 안내의 `x forget`을 `x forget(known_host)`로 바꾼다.
4. 삭제 확인과 호스트 키 삭제 확인에서 기본 선택을 취소로 한다(`Enter`를 연타해도 지워지지 않음).

## 대상 파일

- `src/app/backend.rs`, `src/app/mod.rs`, `src/app/list.rs`, `src/app/confirm.rs`, `src/app/tests.rs`
- `src/system.rs`, `src/main.rs`
- `src/ui/list.rs`, `src/ui/mod.rs`, `src/ui/overlays.rs`, `src/ui/hover.rs`, `src/ui/tests.rs`
- `src/tui.rs`(클릭과 호버 대상 추가에 필요할 때만)

## 1. 마지막 접속 호스트

`Backend`에 기본 구현이 있는 메서드 추가:

```rust
/// Returns the alias that was connected most recently, if recorded.
fn last_host(&self) -> Option<String> { None }
/// Records that a saved alias is about to be connected. Failures are ignored.
fn record_connection(&self, _alias: &str) {}
```

- `SystemBackend`: `with_state_file(self, path: Option<PathBuf>) -> Self` 추가(기본 `None`이면 읽지도 쓰지도 않음). `last_host`는 `state::read_last_host`, `record_connection`은 `state::write_last_host`(오류 무시).
- `src/main.rs`: `SystemBackend`에 `state::default_state_file()`을 넘긴다.
- `App::new`: config를 읽은 뒤 `backend.last_host()`가 목록에 있는 별칭이면 그 줄을 선택. 없으면 지금처럼 첫 줄. 시작할 때 한 번만 적용한다(이후 재로드에는 적용하지 않음).
- 저장된 별칭으로 접속이 확정될 때(`Exit::Connect`를 설정하는 시점, `ConnectTarget::Alias`인 경우) `backend.record_connection(alias)` 호출. 1회 접속(`Direct`)과 인자 오류로 접속하지 못한 경우는 기록하지 않는다.
- 테스트는 `tempfile` 경로만 쓴다. `default_state_file()`은 테스트에서 호출하지 않는다.

## 2. 새 접속 줄

목록의 호스트 줄 뒤에 항상 한 줄을 더 둔다(필터 결과가 0건이어도 표시). 이하 "new 줄".

상태(`src/app/`):

- 선택 위치는 호스트 줄 0..N-1과 new 줄(N). `App::selection()`은 지금처럼 호스트 줄의 인덱스만 반환하고 new 줄이 선택되면 `None`. `pub fn new_row_selected(&self) -> bool` 추가.
- `↓`, `j`, 휠 아래: 마지막 호스트 다음은 new 줄. `↑`, `k`: new 줄에서 마지막 호스트로. `End`, `G`는 지금처럼 마지막 호스트, `Home`, `g`는 첫 호스트.
- 필터 결과가 0건이면 new 줄이 선택된 상태.
- new 줄에서 `Enter` → 새 접속 폼(`n`과 동일). `e`, `d`, `x`는 아무 일도 하지 않음(상태 메시지 없음). `n`은 어디서나 지금처럼 동작.
- `selected_command()`, `selected_effective()`는 new 줄에서 `None`.
- 클릭: `ClickTarget::NewRow` 추가. 한 번 클릭으로 new 줄을 선택하고 새 접속 폼을 연다. 호버 대상 `HoverKey::NewRow` 추가(호스트 줄과 같은 방식의 배경 변화).

화면(`src/ui/list.rs`):

- new 줄은 호스트 줄과 같은 스크롤 영역의 마지막 줄. 내용은 왼쪽에 `+ New connection`, 같은 줄의 요약 열 위치에 흐린 색 `n`. 선택되면 호스트 줄과 같은 선택 표시(`›`, 선택 배경).
- 필터 결과 0건: 첫 줄에 `No matching hosts`, 그 아래 줄에 new 줄.
- new 줄이 선택된 동안 상세 열에는 `New connection`(굵게)과 `Add a host or connect once without saving.`(흐린 색, 폭에 맞춰 줄바꿈)만 표시. `enter runs` 영역은 표시하지 않음.
- 머리줄의 호스트 수 표기는 그대로.

키 안내(목록, 필터 입력 중이 아닐 때): `n new`를 빼고 다음 7쌍으로 한다. `n` 키는 계속 동작한다.

```
enter connect   e edit   d delete   x forget(known_host)   / filter   r reload   esc quit
```

- new 줄이 선택된 동안 첫 쌍은 `enter new`.
- 첫 쌍(배지)은 호스트 줄이나 new 줄이 선택되어 있으면 활성.
- 필터 적용 상태의 마지막 쌍은 지금처럼 `esc clear filter`. 넘침 규칙은 작업 22의 것을 그대로 쓴다(폭 100에서도 필터 적용 상태는 `r reload`가 빠짐).

## 3. 삭제와 호스트 키 삭제 확인의 기본값

`ConfirmKind::Delete`와 `ConfirmKind::Forget`에만 적용한다. `ConfirmKind::Save`는 지금과 같다(`Enter`와 `y`로 저장).

상태(`src/app/confirm.rs`):

```rust
/// The highlighted answer on a destructive confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice { Cancel, Proceed }

/// Returns the highlighted answer, or None when the confirmation has no choice row.
pub fn choice(&self) -> Option<ConfirmChoice>;
```

- Delete와 Forget은 `Some(ConfirmChoice::Cancel)`로 시작. Save는 `None`. `ConfirmChoice`는 `crate::app`에서 재공개.
- 키(Delete, Forget): `←`, `→`, `Tab`, `Shift+Tab`, `h`, `l`은 선택 전환. `Enter`는 강조된 선택 실행(Cancel이면 목록 복귀, Proceed이면 기존 확정 동작). `y`는 선택과 무관하게 바로 확정. `n`, `Esc`는 취소. `↑`, `↓`, `PageUp`, `PageDown` 스크롤은 그대로.

화면(`src/ui/overlays.rs`):

- 본문 맨 아래 줄에 질문 줄을 둔다. 백업 안내 줄은 그 바로 위 줄로 올린다. 높이가 부족하면 백업 안내 줄을 먼저 생략하고 질문 줄은 유지한다.
- 질문 문구: Delete는 `Delete {alias} from {path}?`(`path`는 홈 축약 경로), Forget은 `Remove 1 known_hosts entry for {name}?` 또는 `Remove {n} known_hosts entries for {name}?`.
- 질문 뒤에 두 선택지를 3칸 간격으로 표시: `Cancel`과 `Delete`(Forget에서는 `Forget`). 강조된 선택지는 양옆에 공백을 둔 배지 모양(키 안내의 첫 쌍과 같은 방식)으로 그리고, Proceed가 강조될 때는 오류 색 계열 배지로 그린다. 강조되지 않은 선택지는 흐린 색. 16색 모드에서도 어느 쪽이 강조되었는지 구분되어야 한다.
- 질문이 폭을 넘으면 질문 문구를 잘라 선택지가 항상 보이게 한다.
- 클릭: `Cancel`은 `ClickTarget::Key(KeyCode::Char('n'))`, `Delete`/`Forget`은 `ClickTarget::Key(KeyCode::Char('y'))`.
- 키 안내(Delete): ` enter  cancel   ←→ choose   y delete   n back to list   ↑↓ scroll`. 첫 쌍의 설명은 강조된 선택에 따라 `cancel` 또는 `delete`. Forget은 `delete` 자리에 `forget`. `←→ choose`는 클릭 대상이 아님.

## 필수 테스트

`src/app/tests.rs`:

- `last_host`가 목록의 3번째 별칭이면 시작 선택이 그 줄, 없는 별칭이나 `None`이면 첫 줄
- 목록에서 `Enter`, 더블 클릭, 저장 후 접속 각각에서 `record_connection`이 그 별칭으로 1회 호출. 1회 접속과 접속 실패에서는 호출 없음
- new 줄: `↓`로 진입과 `↑`로 복귀, `End`는 마지막 호스트, new 줄에서 `selection() == None`과 `new_row_selected()`, `Enter`로 폼 열림, `e`/`d`/`x` 무동작, 필터 0건일 때 new 줄 선택, `ClickTarget::NewRow` 한 번 클릭으로 폼 열림
- Delete 확인: 시작 선택 Cancel, `Enter`만 누르면 목록 복귀이고 `commit` 호출 없음, `→` 후 `Enter`로 삭제, `y`로 바로 삭제, `Tab`과 `h`/`l` 전환, `n`과 `Esc` 취소
- Forget 확인: 같은 항목
- Save 확인: `choice() == None`, `Enter`로 저장(기존 동작 유지)

`src/system.rs`: `with_state_file`로 임시 파일을 주고 `record_connection` 후 `last_host` 왕복, 상태 파일 없이 만든 백엔드는 `None`이고 파일을 만들지 않음.

`src/ui/tests.rs`:

- 목록 키 안내 전체 문자열(폭 100): ` enter  connect   e edit   d delete   x forget(known_host)   / filter   r reload   esc quit`
- 폭 96에서 필터 없는 목록은 7쌍 모두 표시, 필터 적용 상태는 `r reload` 없이 `esc clear filter`로 끝남
- new 줄: 호스트 줄 바로 아래에 `+ New connection`, 선택 시 `›`와 상세 열 문구, 첫 쌍 `enter new`, 클릭 대상 `ClickTarget::NewRow`, 필터 0건 화면
- Delete 확인 화면(100x28): 질문 줄 위치(본문 마지막 줄)와 문구, 백업 안내가 그 위 줄, 시작 시 `Cancel` 강조(스타일로 확인), `→` 후 `Delete` 강조, 두 선택지의 클릭 대상, 키 안내 문자열
- Forget 확인 화면: 질문 문구(단수와 복수)와 `Forget` 선택지

## 기존 테스트 수정 허용 범위

- 목록 키 안내 기대 문자열(`n new` 제거, `forget(known_host)`)
- 삭제나 호스트 키 삭제를 `Enter`로 확정하던 테스트: 확정 키를 `y`로 바꾸거나 선택 전환을 추가(검증 내용은 유지)
- 확인 화면의 백업 안내 줄 위치, new 줄 추가로 달라지는 줄 위치와 휠, `↓` 끝 동작의 기대값

이 범위의 수정만 허용하고, 고친 테스트 이름과 이유를 최종 보고에 전부 나열한다. 그 밖의 기존 단언은 바꾸지 않는다.

## 제약

- 실제 홈 디렉터리의 상태 파일과 실제 `~/.ssh` 접근 금지.
- `Cargo.toml` 수정 금지.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- `git status`에 대상 파일만 변경으로 표시
