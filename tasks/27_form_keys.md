# 작업 27: 접속 폼의 키 목록 상시 표시, 키 로그인 설정, 비밀번호 입력 끄기

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 작업 25의 결과(`OptionKey::PasswordAuthentication`, `ConnectTarget::Direct::disable_password`, `ssh::keysetup`, `ssh::exec::run_with_input`)를 사용한다.

## 목표

1. 접속 폼(새 접속과 편집 모두)에서 `ctrl+l` 팝업을 열지 않아도 ssh 디렉터리의 키 목록이 `Key file` 필드 아래에 보이고, 바로 고를 수 있다.
2. 폼에 체크 항목 2개를 추가한다.
   - `Key login`: 서버의 `authorized_keys`에 공개 키를 설치(이후 키로 로그인)
   - `Password`: 이 호스트에서 비밀번호 입력 끄기(권장). config의 `PasswordAuthentication no`
3. 공개 키 설치는 TUI를 잠시 내려놓고 시스템 `ssh`를 실행한다(서버 비밀번호 입력이 필요하므로).

## 대상 파일

- `src/app/backend.rs`, `src/app/mod.rs`, `src/app/form.rs`, `src/app/confirm.rs`, `src/app/tests.rs`
- `src/system.rs`, `src/tui.rs`
- `src/ui/form.rs`, `src/ui/mod.rs`, `src/ui/hover.rs`, `src/ui/tests.rs`

## 1. 필드와 포커스 순서

`FormField`에 `InstallKey`, `DisablePassword`를 추가한다. 포커스 순서는 `Nickname`, `User`, `Address`, `Port`, `KeyFile`, `InstallKey`, `DisablePassword`. 뒤의 둘은 글자 입력이 없는 체크 항목이다.

- `FormState`에 `pub fn is_checked(&self, field: FormField) -> bool`(체크 항목의 상태) 추가.
- 체크 항목에 포커스가 있을 때: `Space`는 전환, 그 밖의 글자, 붙여넣기, `ctrl+u`는 무시. `Tab`, `Shift+Tab`, `↑`, `↓`는 필드 이동. `Enter`는 지금처럼 제출.
- 클릭: 체크 항목의 줄을 클릭하면 포커스 이동과 전환.
- `InstallKey`는 항상 꺼진 상태로 시작한다(설정값이 아니라 이번 제출에서 수행할 동작).
- `DisablePassword`의 시작 상태: 새 접속은 꺼짐. 편집은 블록의 `PasswordAuthentication`이 값 하나이고 `no`(대소문자 무시)이면 켜짐. `Unrepresentable`이면 다른 읽기 전용 필드와 같은 방식으로 원본 줄을 보여 주고 전환 불가.

## 2. 키 목록 상시 표시

폼을 열 때마다(새 접속, 편집, 히스토리 등 모든 진입 경로) `backend.discover_keys()` 결과를 `FormState`에 넣는다. `pub fn keys(&self) -> &[KeyInfo]`, `pub fn key_cursor(&self) -> Option<usize>` 추가.

키 조작(포커스가 `KeyFile`이고 읽기 전용이 아니며 키가 1개 이상일 때):

- 필드에 들어오면 커서는 항상 `None`(입력칸).
- `↓`: `None`이면 0번 키로, `Some(i)`이면 `i+1`로. 이동할 때마다 `Key file` 값이 그 키의 홈 축약 경로(`paths.abbreviate_home`)로 바뀐다. 마지막 키에서 `↓`는 다음 필드(`InstallKey`)로 이동.
- `↑`: `Some(i>0)`이면 `i-1`로(값도 변경), `Some(0)`이면 `None`(값 유지), `None`이면 이전 필드(`Port`)로 이동.
- 글자 입력, 붙여넣기, `ctrl+u`는 지금처럼 입력칸을 편집하고 커서를 `None`으로 되돌린다.
- `Tab`, `Shift+Tab`은 항상 필드 이동.
- 키 줄 클릭(`ClickTarget::KeyRow(i)`, `Mode::Form`에서): 포커스를 `KeyFile`로 옮기고 커서 `Some(i)`, 값 설정.
- 키가 없거나 필드가 읽기 전용이면 `↑`, `↓`는 지금처럼 필드 이동.
- `ctrl+l`/`F2` 팝업과 `ctrl+o`/`F3`는 그대로 동작한다.

화면:

- `Key file` 줄(검증 오류 줄이 있으면 그 아래)에 키 줄을 최대 4개 표시. 입력칸 열에 맞춰 들여쓴다.
- 줄 구성: 표시 1칸, 홈 축약 경로, 키 종류와 코멘트(기존 키 목록 팝업의 표기 방식 재사용). 표시는 커서가 있는 줄 `›`(강조색, 선택 배경), 커서는 없지만 현재 `Key file` 값과 경로가 같은 줄 `●`, 그 외 공백. 값과 같은 줄은 기본 글자색, 나머지는 흐린 색.
- 키가 표시 줄 수보다 많으면 커서 줄(커서가 없으면 값과 같은 줄)이 보이도록 창을 옮기고, 아래에 가려진 키가 있으면 마지막 표시 줄 다음에 흐린 색 `… N more` 한 줄을 둔다.
- 팝업 뒤에 깔린 폼(`theme.dim`), 읽기 전용 `Key file`, 키 0개에서는 표시하지 않는다.
- 호버: 팝업의 키 줄과 같은 `HoverKey::KeyRow(i)`.

세로 배치 규칙(공간이 부족할 때의 우선순위):

1. 필드 5줄과 체크 항목 2줄은 항상 표시(지금의 스크롤 규칙 유지).
2. `On enter` 안내 영역(구분선 포함 5줄)을 확보한다.
3. 남는 줄에 키 줄을 최대 4개(와 `… N more`) 배치한다.
4. 그러고도 남으면 지금처럼 텍스트 필드 사이에 빈 줄을 둔다. 체크 항목 2줄은 서로 붙이고, `Key file`(과 키 줄) 다음에 빈 줄 하나를 둔다. 빈 줄을 넣으면 2, 3이 들어가지 않는 높이에서는 빈 줄을 모두 뺀다.

체크 항목의 표시:

- 라벨: `Key login`, `Password`(다른 필드와 같은 라벨 열과 포커스 표시 `›`).
- 값: `[ ] Install the public key on the server`, `[ ] Disable password prompt (recommended)`. 켜지면 `[x]`. 입력칸 배경 없이 일반 글자로 그린다.
- 안내(값 뒤 2칸, 폭이 남을 때만): `Key login`은 흐린 색 `asks for the server password once`. `Password`는 켜져 있고 `Key login`이 꺼져 있을 때 경고색 `make sure key login already works`.

키 안내(맨 아래):

- `KeyFile` 포커스: 첫 쌍, `tab next`, `↑↓ pick key`(클릭 대상 아님), `ctrl+o browse`, `esc back`. 기존의 `ctrl+l pick key` 안내는 이것으로 바꾼다.
- 체크 항목 포커스: 첫 쌍, `tab next`, `space toggle`(클릭하면 전환), `esc back`.

## 3. 비밀번호 입력 끄기의 저장 규칙

- 새 호스트 저장: 켜져 있으면 `NewHost::password_authentication = Some("no")`.
- 편집: 켜짐인데 원래 `no`가 아니면 `Change::Set("no")`, 꺼짐인데 원래 `no`이면 `Change::Remove`, 그 외 `Change::Keep`.
- 닉네임 없는 1회 접속: `ConnectTarget::Direct::disable_password`에 반영. 명령 미리보기에도 나타난다.

## 4. 공개 키 설치

`Backend`에 기본 구현이 있는 메서드 추가:

```rust
/// Reads the public key that belongs to the key file typed in the form.
fn read_public_key(&self, _key_file: &str) -> Result<String, String> {
    Err("public key lookup is unavailable".into())
}
/// Runs ssh to install a public key. Returns the ssh exit code.
fn install_key(&self, _args: &[OsString], _public_key: &str) -> Result<i32, String> {
    Err("key installation is unavailable".into())
}
```

`SystemBackend`: `read_public_key`는 `keysetup::public_key_path`와 `keysetup::read_public_key`, `install_key`는 `exec::run_with_input(ssh, args, "<키 한 줄>\n")`.

앱(`src/app/`):

```rust
/// A key installation that must run outside the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInstall {
    /// Alias or address shown to the user.
    pub label: String,
    /// Arguments for ssh.
    pub args: Vec<OsString>,
    /// The public key line sent on stdin.
    pub public_key: String,
}

/// Returns the installation waiting to run, once.
pub fn take_key_install(&mut self) -> Option<KeyInstall>;
/// Reports the result of the installation taken earlier.
pub fn finish_key_install(&mut self, result: Result<i32, String>);
```

제출(`Enter`)에서 `InstallKey`가 켜져 있을 때:

1. 기존 검증을 먼저 통과해야 한다.
2. 사용할 키 파일은 `Key file` 값, 비어 있으면 그 필드의 상속값(플레이스홀더). 둘 다 없으면 상태 메시지(Error) `Key login needs a key file`, 포커스를 `KeyFile`로 옮기고 폼 유지.
3. `backend.read_public_key(key_file)` 실패 시 그 오류를 상태 메시지(Error)로 띄우고 포커스를 `KeyFile`로 옮기고 폼 유지. 이 시점까지 아무것도 쓰지 않는다.
4. 종류별 진행. 설치 인자는 `keysetup::install_args`로 만든다.
   - 새 접속, 닉네임 없음: 설치 실행 → 성공하면 지금과 같은 1회 접속(`Exit::Connect`), 실패하면 폼 유지와 Error 메시지.
   - 새 접속, 닉네임 있음: 지금처럼 config에 저장(저장 실패 처리는 기존과 동일) → 설치 실행(대상은 별칭) → 성공하면 기존 저장 후 흐름(불일치 알림 또는 접속), 실패하면 목록(저장된 호스트 선택)과 Error 메시지.
   - 편집: config 변경이 있으면 지금처럼 변경 확인 화면 → 확정 후 설치 실행. 변경이 없으면 `No changes` 대신 바로 설치 실행. 설치 뒤에는 목록으로 돌아가고, 성공이면 Info `Installed the public key on {label}`(편집에서 불일치가 있으면 기존처럼 알림 표시), 실패면 Error.
5. 결과 판정: `Ok(0)`은 성공. `Ok(code)`는 `Key install failed for {label} (ssh exit code {code})`. `Err(error)`는 `Key install failed for {label}: {error}`.
6. `label`은 별칭 또는 1회 접속의 주소.

"설치 실행"은 앱이 `KeyInstall`을 대기 상태로 두는 것이고, 실제 실행은 `src/tui.rs`의 루프가 한다. 대기 중에는 키와 마우스 입력을 처리하지 않는다.

`InstallKey`가 꺼져 있으면 제출 동작은 지금과 완전히 같다.

`On enter` 안내: `InstallKey`가 켜져 있으면 단계 한 줄을 추가한다. 새 접속은 `Save` 다음, `Run` 앞에 `Key   install <공개 키의 홈 축약 경로> on the server`(번호는 순서대로 다시 매김). 편집은 `1 Review changes to ...` 다음 줄에 `2 Key   install ... on the server`.

## 5. TUI 일시 중단 (`src/tui.rs`)

이벤트를 처리한 뒤 `app.take_key_install()`이 `Some`이면:

1. 종료할 때와 같은 방식으로 마우스 캡처를 끄고 남은 입력을 비운 뒤 터미널을 복원한다(대체 화면 해제, raw 모드 해제, 커서 표시). 남은 마우스 입력이 ssh의 비밀번호 입력으로 들어가면 안 된다.
2. 표준 출력에 `sshtui: installing the public key on {label}`과 `sshtui: the server may ask for your password`를 출력한다.
3. `app.backend().install_key(&args, &public_key)`를 실행한다.
4. 결과 한 줄(`sshtui: key install finished` 또는 `sshtui: key install failed`)과 `Press Enter to return to sshtui.`를 출력하고 표준 입력에서 한 줄을 기다린다(ssh의 출력을 읽을 시간).
5. 터미널을 다시 초기화한다(raw 모드, 대체 화면, 붙여넣기 모드, 옵션에 따른 마우스 캡처). 화면 전체를 다시 그린다.
6. `app.finish_key_install(result)` 호출.

중단과 재개는 함수로 분리하고, 패닉 훅과 `Drop` 복원이 중단 구간에서도 터미널을 망가뜨리지 않도록 한다. 공개 키 내용은 출력하지 않는다.

## 필수 테스트

`src/app/tests.rs`(가짜 백엔드에 키, 공개 키 결과, 설치 결과 주입과 호출 기록 추가):

- 포커스 순서 7개와 순환, 체크 항목의 `Space` 전환과 글자 입력 무시, 클릭 전환
- 키 목록 탐색: `↓`로 0번 선택과 값 변경, 마지막 키에서 `↓`로 `InstallKey` 이동, `↑`로 되돌아가 `None`에서 `Port`로 이동, 입력 시 커서 해제, `ClickTarget::KeyRow` 클릭, 키 0개일 때 기존 이동
- `DisablePassword`: 새 호스트 저장 내용에 `PasswordAuthentication no` 포함, 편집의 3가지 경우(추가, 제거, 유지)의 변경 확인 diff, 1회 접속 인자에 `-o PasswordAuthentication=no`, 편집 폼의 시작 상태, `Unrepresentable` 읽기 전용
- `InstallKey`: 키 파일 없음 오류, `read_public_key` 실패 시 아무것도 쓰지 않음, 4번의 세 경로 각각에서 `take_key_install`의 `label`, `args`(끝이 `REMOTE_INSTALL_SCRIPT`, `PasswordAuthentication=yes` 포함), `public_key`, 그리고 `finish_key_install`의 성공과 실패(`Ok(1)`, `Err`) 뒤 상태(모드, 메시지, `Exit`), 대기 중 키 입력 무시, `take_key_install`이 한 번만 반환
- `InstallKey`가 꺼져 있을 때 기존 제출 테스트가 그대로 통과

`src/system.rs`: `read_public_key`(임시 ssh 디렉터리의 `.pub`), `#[cfg(unix)]`에서 가짜 ssh 스크립트로 `install_key`의 표준 입력 내용과 종료 코드.

`src/ui/tests.rs`:

- 100x28 새 접속 폼: 체크 항목 2줄의 위치와 문구, 켠 상태 `[x]`, 포커스 표시, 안내 문구(경고 포함)
- 키 2개: `Key file` 아래 키 줄 2개, 커서 줄 `›`, 값과 같은 줄 `●`, 클릭 대상 `ClickTarget::KeyRow`
- 키 6개: 4줄과 `… 2 more`, 커서 이동 시 창 이동
- 80x24: 빈 줄 없는 배치에서 필드 5줄, 키 줄, 체크 항목 2줄, `On enter` 영역이 모두 보임
- 팝업 뒤 폼에는 키 줄 없음
- 키 안내 문자열 3종(`KeyFile` 포커스, 체크 항목 포커스, 그 외)
- `On enter`의 `Key   install ... on the server` 단계(새 접속과 편집)

## 기존 테스트 수정 허용 범위

- 포커스 순환 끝이 `KeyFile`에서 `Nickname`으로 돌아간다고 가정한 테스트(이제 체크 항목을 거침)
- `KeyFile` 포커스의 키 안내 문자열(`ctrl+l pick key` → `↑↓ pick key`)
- 체크 항목 2줄 추가로 달라지는 폼의 줄 위치(구분선, `On enter` 영역)
- 가짜 백엔드가 키를 갖고 있을 때 `KeyFile`에서의 `↑`, `↓` 기대 동작

이 범위의 수정만 허용하고, 고친 테스트 이름과 이유를 최종 보고에 전부 나열한다.

## 제약

- 실제 `~/.ssh`와 실제 홈 접근 금지. 테스트에서 실제 `ssh`로 원격 접속을 시도하지 않는다.
- 공개 키와 비밀키의 내용을 화면, 상태 메시지, 로그에 넣지 않는다(공개 키는 ssh의 표준 입력으로만 전달).
- `Cargo.toml` 수정 금지.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- `git status`에 대상 파일만 변경으로 표시
