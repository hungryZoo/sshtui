# 작업 28: 셸 히스토리 후보 가져오기 화면

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 작업 24의 `src/history.rs`(`history_sources`, `read_ssh_commands`, `suggest`, `Suggestion`, `SavedHost`)를 화면에 연결한다. `src/history.rs`의 동작은 바꾸지 않는다.

## 목표

셸 히스토리의 `ssh` 명령 가운데 config에 아직 없는 접속 대상을 후보로 보여 주고, 고르면 값이 채워진 새 접속 폼을 연다. 시작할 때 묻지 않는다. 후보가 있을 때만 머리줄에 알림을 표시하고 `i`로 연다.

## 대상 파일

- `src/app/backend.rs`, `src/app/mod.rs`, `src/app/form.rs`, `src/app/tests.rs`, `src/app/import.rs`(신규, 필요하면)
- `src/system.rs`, `src/main.rs`
- `src/ui/list.rs`, `src/ui/mod.rs`, `src/ui/overlays.rs`, `src/ui/hover.rs`, `src/ui/tests.rs`

## 1. Backend

```rust
/// Returns ssh commands from shell history, oldest first. Called once at startup.
fn recent_ssh_commands(&self) -> Vec<String> { Vec::new() }
```

- `SystemBackend`: `with_history_sources(self, sources: Vec<HistorySource>) -> Self` 추가(기본은 빈 목록이며 아무 파일도 읽지 않음). `recent_ssh_commands`는 `history::read_ssh_commands(&self.history_sources)`.
- `src/main.rs`: `history::history_sources(dirs::home_dir().as_deref(), dirs::config_dir().as_deref(), &|name| std::env::var_os(name))` 결과를 넘긴다. `--help` 출력에 `SSHTUI_NO_HISTORY`와 `SSHTUI_HISTORY_FILE` 설명 한 줄씩 추가.
- 테스트는 임시 파일만 쓴다. 실제 히스토리 파일을 읽는 테스트 금지.

## 2. 앱 상태

- `App::new`에서 `backend.recent_ssh_commands()`를 한 번 호출해 보관한다.
- config를 읽을 때마다(시작, 재로드, 저장 뒤) 후보를 다시 계산한다: `history::suggest(commands, saved)`. `saved`는 각 호스트의 별칭과 그 블록에 직접 적힌 `HostName` 값.
- `pub fn suggestions(&self) -> &[Suggestion]` 추가.
- `Mode::Import(ImportState)` 추가. `ImportState`는 선택 인덱스를 갖고 `pub fn selection(&self) -> usize`를 제공. 후보 목록은 `app.suggestions()`에서 읽는다.
- 목록에서 `i`(필터 입력 중이 아닐 때): 후보가 있으면 `Mode::Import`(선택 0), 없으면 상태 메시지(Info) `No new ssh destinations in shell history`.
- `Mode::Import`의 키: `↑`/`k`, `↓`/`j` 선택 이동(끝에서 멈춤), `Enter`는 선택한 후보로 새 접속 폼 열기, `Esc`는 목록 복귀. 휠은 선택 이동.
- 후보로 연 폼: `User`, `Address`, `Port`, `Key file`에 후보 값(없는 값은 빈 칸), `Nickname`은 비어 있고 포커스는 `Nickname`. 그 뒤 동작은 일반 새 접속 폼과 같다(`Esc`는 목록으로).
- 클릭: `ClickTarget::SuggestionRow(usize)` 추가. 후보 줄 클릭은 선택 후 `Enter`와 같은 동작. 팝업 바깥 클릭(`ClickTarget::Outside`)은 `Esc`. 호버 대상 `HoverKey::SuggestionRow(usize)`.
- 후보를 폼으로 열어 닉네임을 붙여 저장하면 재로드 때 그 후보가 사라져야 한다(저장된 `HostName`과 주소가 같으므로).

## 3. 화면

머리줄 알림(`src/ui/list.rs`의 `header`):

- 후보가 1개 이상이면 `1 suggestion i` 또는 `{n} suggestions i`를 머리줄 오른쪽에 표시(`i`는 굵게, 업데이트 알림과 같은 구성이되 색은 강조색). 클릭 대상은 `ClickTarget::Key(KeyCode::Char('i'))`.
- 업데이트 알림이 함께 있으면 `{n} suggestions i   update available u` 순서로 3칸 띄워 나란히 표시. 폭이 부족하면 후보 알림을 먼저 생략한다.
- 팝업이 떠 있는 동안의 머리줄(흐려진 배경)에는 지금 규칙을 그대로 적용.

팝업(`src/ui/overlays.rs`, 키 목록 팝업과 같은 상자 모양과 흐려진 배경):

- 제목 `Import from shell history`, 제목 줄 오른쪽에 `{n} found`.
- 줄 구성: 선택 표시 `›`, 대상(`user@address`, 사용자 없으면 `address`, 포트가 있으면 뒤에 `:port`), 키 파일(있을 때, 흐린 색), 오른쪽 끝에 `×{count}`(흐린 색).
- 한 번에 최대 10줄, 선택이 보이도록 스크롤.
- 팝업 아래쪽에 흐린 색 안내 한 줄: `Pick one to open the new connection form with these values.`
- 키 안내: ` enter  fill form   ↑↓ select   esc close`

## 필수 테스트

`src/app/tests.rs`(가짜 백엔드에 히스토리 명령 주입):

- 후보 계산: 저장된 별칭과 `HostName`은 제외, 최신순, 재로드 뒤 갱신
- `recent_ssh_commands`는 시작할 때 한 번만 호출
- `i`: 후보 없음 메시지, 후보 있음 → `Mode::Import`
- 팝업: 선택 이동과 끝 멈춤, `Esc`, `Enter`로 폼이 열리고 4개 필드 값과 포커스 확인, 포트와 키 파일이 없는 후보
- 후보 폼에서 닉네임을 넣고 저장한 뒤 그 후보가 목록에서 사라짐
- `ClickTarget::SuggestionRow` 클릭, 바깥 클릭, 휠
- 필터 입력 중 `i`는 필터 글자로 입력

`src/system.rs`: `with_history_sources`에 임시 zsh 형식 파일을 주고 ssh 명령만 돌아오는지, 기본 백엔드는 빈 목록.

`src/ui/tests.rs`:

- 머리줄 알림: 단수와 복수, 업데이트 알림과 함께 있을 때의 순서, 좁은 폭에서 후보 알림 생략, 클릭 대상
- 팝업(100x28): 제목과 `{n} found`, 줄 구성(사용자, 포트, 키 파일, 횟수), 선택 표시, 안내 줄, 키 안내 문자열, 줄 클릭 대상, 12개 후보에서 10줄 표시와 스크롤

## 기존 테스트 수정 허용 범위

없음. 기존 단언은 바꾸지 않는다. 충돌하면 고치지 말고 최종 보고에 적는다.

## 제약

- 히스토리의 명령 문자열 자체는 화면에 표시하지 않는다(추출한 접속 값만 표시). 로그와 상태 메시지에도 넣지 않는다.
- 실제 홈의 히스토리 파일과 실제 `~/.ssh` 접근 금지.
- `Cargo.toml` 수정 금지.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- `git status`에 대상 파일만 변경으로 표시
