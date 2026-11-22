# 작업 22: 호스트 목록에서 known_hosts 항목 삭제 (화면 연결)

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 작업 21에서 만든 `src/known_hosts/`(`lookup_name_for`, `file_path`, `plan_forget`, `ForgetPlan`)를 화면에 연결한다. `src/known_hosts/`의 동작은 바꾸지 않는다.

## 목표

호스트 목록에서 `x`를 누르면 선택한 호스트의 호스트 키 항목을 `known_hosts`에서 찾아 변경 확인 화면에 보여 주고, 승인하면 지운다. 기존 변경 확인 화면(`Mode::Confirm`)과 쓰기 경로(`Backend::commit`, 백업, 변경 감지)를 그대로 쓴다.

## 대상 파일

- `src/app/backend.rs`, `src/app/mod.rs`, `src/app/confirm.rs`, `src/app/tests.rs`
- `src/system.rs`
- `src/ui/list.rs`, `src/ui/mod.rs`, `src/ui/overlays.rs`, `src/ui/tests.rs`

## 1. Backend

`Backend` 트레이트에 기본 구현이 있는 메서드 추가:

```rust
/// Plans removing the known_hosts entries stored under `name`.
/// Returns Ok(None) when the file is missing or has no entry for the name.
fn plan_forget(&self, name: &str) -> Result<Option<ForgetPlan>, String> {
    Ok(None)
}
```

`SystemBackend`는 `known_hosts::plan_forget(&self.paths, name)`에 위임하고 오류는 `to_string()`으로 바꾼다. `src/system.rs` 테스트 추가: 임시 ssh 디렉터리에 `known_hosts`를 쓰고 `plan_forget` → `commit` 후 파일과 백업 내용 확인, 파일이 없을 때 `Ok(None)`.

## 2. 앱 상태 (`src/app/`)

### 키

`handle_list_key`의 필터 입력 중이 아닌 분기에 `KeyCode::Char('x')` 추가. 필터 입력 중의 `x`는 지금처럼 필터 글자로 들어간다.

### 동작 (`forget_selected`)

1. 선택된 줄이 없으면 아무 일도 하지 않음. 읽기 전용 config 파일의 호스트도 허용(`writable_selection`을 쓰지 않음).
2. `self.list.effective(&self.backend, &alias)`(캐시 사용)가 `Err(error)`이면 상태 메시지(Error) `Cannot resolve {alias}: {error}`를 띄우고 목록 유지. 이때 `plan_forget`을 호출하지 않는다.
3. `name = known_hosts::lookup_name_for(&alias, effective)`
4. `self.backend.plan_forget(&name)`
   - `Err(error)` → 상태 메시지(Error) `error`
   - `Ok(None)` → 상태 메시지(Info) `No entry for {name} in {path}`. `path`는 `paths.abbreviate_home(&known_hosts::file_path(paths))`
   - `Ok(Some(plan))` → `Mode::Confirm(ConfirmState::new(plan.pending, alias, ConfirmAction::Forget { name: plan.name, removed: plan.removed }))`

### ConfirmState

- `ConfirmAction`에 `Forget { name: String, removed: usize }` 추가.
- 공개 열거형과 접근자 추가. 기존 `is_delete()`는 그대로 두고(Forget에서는 `false`) 유지한다.

  ```rust
  /// What a confirmation will do when accepted.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum ConfirmKind { Save, Delete, Forget }

  /// Returns the kind of change awaiting confirmation.
  pub fn kind(&self) -> ConfirmKind;
  /// Returns the known_hosts lookup name when host key entries will be removed.
  pub fn forget_name(&self) -> Option<&str>;
  ```

  `ConfirmKind`는 `crate::app`에서 재공개한다.
- `title()`: Forget이면 `Forget host key for {alias}`.

### 확인 화면의 키

- `n`, `Esc`: Forget이면 `Mode::List`(아무것도 쓰지 않음).
- `y`, `Enter`: `self.backend.commit(&pending)` 호출 후 Forget인 경우
  - `Ok(outcome)`: config를 다시 읽지 않는다(`reload` 호출 없음, 선택 유지). 상태 메시지(Info): `removed == 1`이면 `Removed 1 known_hosts entry for {name} (backup: {backup})`, 아니면 `Removed {removed} known_hosts entries for {name} (backup: {backup})`. `backup` 표기는 기존 Saved/Deleted 메시지와 같은 방식(`none` 포함).
  - `Err(WriteError::Stale { path })`: config를 다시 읽지 않고 상태 메시지(Error) `{path} was changed by another program; press x to try again`
  - 그 외 오류: `error.to_string()`
  - 모든 경우 `Mode::List`로 복귀.
- 기존 Edit, Delete 경로의 동작과 메시지는 바꾸지 않는다.

## 3. 화면 (`src/ui/`)

### 목록의 키 안내

`list_hints`에서 `d delete` 다음, `/ filter` 앞에 `("x", "forget", Some(KeyCode::Char('x')))` 추가(총 8쌍). 클릭과 호버는 기존 키 안내와 같은 방식으로 자동 적용된다.

### 키 안내가 넘칠 때의 규칙 (`render_help`)

지금은 왼쪽부터 그리다가 넘치는 쌍에서 멈추므로 마지막 쌍(`esc ...`)이 먼저 사라진다. 다음 규칙으로 바꾼다.

- 모든 쌍이 들어가면 지금과 같음.
- 넘치고 쌍이 3개 이상이면, 마지막 쌍 바로 앞의 쌍부터 왼쪽 방향으로 하나씩 빼서 전체가 들어갈 때까지 줄인다. 첫 쌍(배지)과 마지막 쌍은 빼지 않는다.
- 첫 쌍과 마지막 쌍만 남겨도 넘치면 기존 방식(왼쪽부터 그리다 넘치면 멈춤)으로 그린다.
- 빠진 쌍은 그리지 않고 클릭 영역도 등록하지 않는다. 쌍 사이 간격(3칸)과 스타일, 호버 처리는 그대로.

결과: 폭 96(안쪽 92칸)에서 필터가 적용된 목록은 `r reload`가 빠지고 `... x forget   / filter   esc clear filter`로 끝난다. 폭 100에서는 8쌍이 모두 보인다.

### 변경 확인 화면 (`overlays::confirm`, `confirm_help`)

- `confirm_help`: `kind()`가 Forget이면 `y forget`, `n back to list`, `↑↓ scroll`.
- 요약 줄: `forget_name()`이 `Some(name)`이면 `1 line will change` 대신 `1 entry for {name} will be removed`(2개 이상은 `{n} entries for {name} will be removed`, `n`은 삭제 줄 수). 뒤의 `  -{removed}` 표시는 그대로.
- 머리줄(제목과 오른쪽 경로), diff, 백업 안내 줄(`The current file is kept as ~/.ssh/.known_hosts.sshtui.bak`)은 기존 코드가 그대로 처리한다. 긴 키 줄은 지금처럼 폭에서 잘린다.

## 필수 테스트

`src/app/tests.rs`(가짜 백엔드에 `plan_forget` 결과 주입과 요청된 이름 기록을 추가):

- `x` → 가짜 백엔드가 받은 이름이 유효 설정에서 계산한 값인지: 포트 22(`host`), 포트 2222(`[host]:2222`), `host_key_alias` 있음(별칭)
- `Ok(None)` → 목록 유지, Info 메시지에 이름과 `known_hosts` 경로 포함
- `Err` → Error 메시지
- 유효 설정 조회 실패 → `Cannot resolve`로 시작하는 Error 메시지, `plan_forget` 호출 없음
- `Ok(Some(plan))` → `Mode::Confirm`, `title()`, `kind() == ConfirmKind::Forget`, `forget_name()`, `path()`, `is_delete() == false`
- 확인 화면에서 `n`과 `Esc` → 목록 복귀, `commit` 호출 없음
- `y`와 `Enter` → `commit`이 그 `pending`으로 1회 호출, 단수와 복수 메시지, 선택 유지, config 재로드 없음(가짜 백엔드의 로드 횟수 불변)
- `commit`이 `Stale` → 위의 Error 메시지, 목록 복귀
- 읽기 전용 config의 호스트에서도 동작
- 필터 입력 중 `x`는 필터 값에 들어가고 `plan_forget` 호출 없음
- `handle_click(ClickTarget::Key(KeyCode::Char('x')), false)`가 키와 같은 동작
- 선택된 줄이 없을 때(필터 결과 0건) `x`는 아무 일도 하지 않음

`src/ui/tests.rs`:

- 목록 키 안내 전체 문자열: ` enter  connect   n new   e edit   d delete   x forget   / filter   r reload   esc quit`
- 폭 100, 필터 적용 상태: 8쌍 모두 표시, `esc clear filter`로 끝남
- 폭 96, 필터 적용 상태: `x forget`, `/ filter` 포함, `r reload` 없음, `esc clear filter`로 끝남. `r reload` 클릭 영역 없음
- 좁은 폭(예: 40): 배지 쌍과 `esc quit`이 함께 보임
- `x forget` 글자 위치의 클릭 대상이 `ClickTarget::Key(KeyCode::Char('x'))`
- Forget 확인 화면(폭 100, 높이 28): 머리줄에 `Forget host key for <별칭>`과 known_hosts 경로, 요약 줄(단수와 복수), 삭제 줄의 `-` 기호, 백업 안내 줄, 키 안내 ` y  forget   n back to list   ↑↓ scroll`

## 기존 테스트 수정 허용 범위

- 목록 키 안내를 통째로 비교하는 기대 문자열에 `x forget`을 넣는 수정
- 넘침 규칙 변경으로 좁은 폭의 키 안내 기대값이 달라지는 경우 그 기대값의 수정

이 두 경우만 허용하고, 고친 단언을 최종 보고에 전부 나열한다. 그 밖의 기존 단언은 바꾸지 않는다.

## 제약

- 실제 `~/.ssh` 접근 금지. 테스트는 `tempfile` 임시 디렉터리만 사용.
- 호스트 키 줄은 확인 화면의 diff에만 나타난다. 상태 메시지와 오류 메시지에 키 내용을 넣지 않는다.
- `Cargo.toml` 수정 금지.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- `git status`에 대상 파일만 변경으로 표시
