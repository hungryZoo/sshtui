# 작업 17: 디자인 목업 반영

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `src/ui/`, `src/app/`, `src/tui.rs`, `src/main.rs`를 읽는다.

## 목표

`docs/design/`의 목업 6장(`Main`, `Filter`, `NewConnection`, `EditError`, `KeyPicker`, `ReviewChanges`의 `.dc.html`)과 같은 모습으로 화면을 다시 그린다. 목업은 HTML이지만 고정폭 글꼴 15px, 줄 높이 24px, 폭 `ch` 단위로 만들어져 있어 터미널 칸에 그대로 대응한다(목업 한 장은 96열 26행). 아래 명세는 목업을 칸 단위로 옮긴 것이며, 명세와 목업이 다르게 읽히면 목업을 따르고 최종 보고에 적는다. 목업 파일은 읽기만 하고 수정하지 않는다.

키 동작과 상태 전이는 바꾸지 않는다. 마우스 지원(`HitMap`)은 유지한다. 현재의 둥근 테두리 패널과 `[ ... ]` 버튼은 없앤다.

## 대상 파일

- `src/ui/` 전체(테마, 배치, 렌더링, 테스트)
- `src/app/`: 렌더러에 필요한 읽기 전용 접근자와 아래 "App에 추가할 정보"만. 상태 전이와 기존 `src/app/tests.rs`의 단언은 수정하지 않는다(테스트 추가는 허용).
- `src/tui.rs`, `src/main.rs`: 색 모드 전달만.

`src/ui/tests.rs`의 기존 테스트는 배치와 문구가 바뀌므로 새 화면에 맞게 고친다. 각 테스트가 확인하던 의도(어떤 정보가 보이는지, 순서, 색, 커서 위치, 클릭 영역, 작은 화면에서 패닉 없음)는 유지하고, 없어진 요소(버튼, 패널 테두리)를 확인하던 테스트는 그 자리를 대신하는 요소(키 안내, 구분선)를 확인하도록 바꾼다. 테스트 수를 줄이지 않는다. 고친 테스트는 최종 보고에 적는다.

## 색 (테마)

`ui::render`가 테마를 인자로 받는다: `pub fn render<B: Backend>(frame: &mut Frame, app: &App<B>, theme: &Theme) -> HitMap`. 색은 `src/ui/theme.rs`에서만 정의한다.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode { TrueColor, Ansi }
pub struct Theme { /* private */ }
impl Theme {
    pub fn new(mode: ColorMode) -> Self;
    /// Same theme with every foreground muted and inputs darkened, for the screen behind a popup.
    pub fn dimmed(&self) -> Self;
}
```

TrueColor 팔레트(목업의 값 그대로, `Color::Rgb`):

| 이름 | 값 | 용도 |
|---|---|---|
| BG | `#15171c` | 화면 전체 배경 |
| TEXT | `#d8dbe1` | 기본 글자 |
| MUTED | `#8d94a0` | 보조 글자 |
| ACCENT | `#6fd3c1` | 강조(제품명, 선택 표시, 명령) |
| WARN | `#e6b566` | `required`, `read-only`, 업데이트 알림 |
| ERROR | `#f08a8a` | 오류, 삭제 줄 |
| OK | `#9ccf82` | 추가 줄, Info 상태 |
| RULE | `#3a404b` | 구분선 |
| INPUT | `#20242c` | 입력칸 배경 |
| INPUT_FOCUS | `#2b313b` | 포커스 입력칸 배경 |
| INPUT_DIM | `#1b1e24` | 팝업 뒤 입력칸 배경 |
| ROW | `#262b34` | 선택된 줄 배경, 비활성 키 배지 배경 |
| DEL_BG | `#3a2226` | 삭제 줄 배경 |
| ADD_BG | `#1f3326` | 추가 줄 배경 |

TrueColor 모드에서는 매 프레임 화면 전체를 BG 배경과 TEXT 글자색으로 먼저 칠한다.

Ansi 팔레트(트루컬러를 지원하지 않는 터미널용): 배경은 칠하지 않는다(`Color::Reset`). TEXT는 Reset, MUTED는 DarkGray, ACCENT는 Cyan, WARN은 Yellow, ERROR는 Red, OK는 Green, RULE은 DarkGray. 입력칸은 배경 대신 밑줄(포커스는 Cyan 밑줄), 선택된 줄은 배경 Cyan과 글자 Black 굵게, 삭제와 추가 줄은 글자색만, 키 배지는 배경 Cyan과 글자 Black 굵게(비활성 배지는 DarkGray 글자).

모드 결정은 `main`에서 한다. 순수 함수 `fn color_mode(colorterm: Option<&OsStr>) -> ColorMode`: `COLORTERM` 값이 `truecolor` 또는 `24bit`(대소문자 무시)이면 `TrueColor`, 아니면 `Ansi`. `tui::Options`에 `color: ColorMode`를 추가해 `run`이 `Theme::new`를 만든다. `src/ui/`에서는 환경변수를 읽지 않는다.

## 공통 틀

- 여백: 화면 폭이 64 이상이면 좌우 2칸, 높이가 18 이상이면 위아래 1줄을 비운다. 아래 좌표는 여백을 뺀 안쪽 영역(폭 `w`, 높이 `h`) 기준이다.
- 0행 머리줄: `sshtui`(ACCENT 굵게), 2칸, 화면 제목. 오른쪽 끝에 화면별 보조 정보.
- 마지막 행 키 안내, 그 바로 위 행 구분선(`─`를 폭 전체에, RULE).
- 상태 메시지(0-3줄, 기존 줄바꿈 규칙)는 아래 구분선 바로 위에 둔다. Info는 OK, Warning은 WARN, Error는 ERROR.
- 테두리 상자는 팝업 외에는 쓰지 않는다. 구분은 `─` 가로선과 `│` 세로선뿐이다.

키 안내 형식:

- 쌍 사이 3칸. 쌍은 `<키><1칸><설명>`.
- 첫 쌍(주 동작)의 키는 배지: 앞뒤 1칸 여백을 포함해 배경 ACCENT, 글자 BG, 굵게(예: ` enter `). 설명은 TEXT.
- 나머지 쌍의 키는 TEXT 굵게, 설명은 MUTED.
- 주 동작을 실행할 수 없는 상태(폼 검증 오류)에서는 배지를 배경 ROW, 글자 MUTED 굵게로, 설명을 MUTED로 그린다.
- 키 표기는 소문자: `enter`, `esc`, `tab`, `ctrl+l`, `ctrl+o`, `ctrl+u`, `↑↓`, 한 글자 키.
- 폭이 부족하면 뒤 쌍부터 생략한다.

## 1. 호스트 목록 (`Main.dc.html`, `Filter.dc.html`)

```
sshtui  12 hosts                                                             update available u

 / filter hosts
────────────────────────────────────────────────────────────────────────────────────────────────
  bastion         ops@bastion.example.com           │  prod-web-1
› prod-web-1      deploy@203.0.113.10               │  deploy@203.0.113.10
  prod-web-2      deploy@203.0.113.11               │
  prod-db         postgres@203.0.113.20             │  User       deploy
  ...                                               │  Address    203.0.113.10
  backup          borg@backup.example.net  read-only│  Port       22
                                                    │  Key file   ~/.ssh/id_ed25519
                                                    │  Source     ~/.ssh/config:14
                                                    │
                                                    │  enter runs
                                                    │  $ ssh prod-web-1
────────────────────────────────────────────────────────────────────────────────────────────────
 enter  connect   n new   e edit   d delete   / filter   r reload   q quit
```

- 머리줄 제목: `12 hosts` 또는 `3 of 12 hosts`(MUTED). 오른쪽: 업데이트 계획이 있으면 `update available`(WARN), 1칸, `u`(WARN 굵게).
- 1행 빈 줄. 2행 필터 바(폭 전체):
  - 평소: 배경 INPUT. 1칸 여백 뒤 `/`(MUTED), 1칸, 필터가 비어 있으면 `filter hosts`(MUTED), 필터가 있으면 필터 문자열(TEXT).
  - 입력 중: 배경 INPUT_FOCUS, `/`는 ACCENT 굵게, 필터 문자열(TEXT), 커서를 문자열 끝 입력 위치에 둔다.
- 3행 구분선. 4행부터 본문.
- 왼쪽 열: 폭은 `w`의 54%이되 최대 52칸. `w`가 64 미만이면 오른쪽 열을 그리지 않고 왼쪽 열이 전체 폭을 쓴다.
  - 줄: 표시 칸 2(선택 줄은 `›` ACCENT 굵게), 이름 칸(폭 16, 가장 긴 이름+2가 더 크면 그만큼, 최대 28), 요약(아래 "App에 추가할 정보"), 읽기 전용이면 오른쪽 끝에 `read-only`(WARN). 열의 오른쪽 1칸은 비운다.
  - 선택 줄: 열 폭 전체 배경 ROW, 이름 TEXT 굵게, 요약 TEXT. 나머지 줄: 이름 TEXT, 요약 MUTED.
  - 필터가 있으면 이름 안에서 필터와 일치하는 부분(대소문자 무시, 첫 일치)을 ACCENT로 그린다. 선택되지 않은 줄에서는 그 부분을 굵게, 선택 줄은 이름 전체가 굵다.
  - 긴 이름과 요약은 칸에 맞춰 자른다. 선택 줄이 보이도록 스크롤(기존 동작).
  - 결과가 없으면 왼쪽 열 첫 줄에 `No matching hosts`(MUTED).
- 세로 구분선: 왼쪽 열 바로 오른쪽 칸에 `│`(RULE)를 본문 높이만큼. 그 뒤 2칸 여백 다음이 오른쪽 열.
- 오른쪽 열(선택 항목의 상세):
  - 0행 별칭(TEXT 굵게), 1행 `user@address`(적용값, MUTED. user가 없으면 주소만), 2행 빈 줄
  - 라벨 칸(폭 11, MUTED)과 값(TEXT): `User`, `Address`, `Port`, `Key file`(여러 개면 라벨은 첫 줄만), `Source`(`<경로>:<줄>`, 홈 하위 `~` 축약, 길면 값 열 안에서 줄바꿈)
  - 읽기 전용이면 `Read-only file`(WARN)
  - 빈 줄, `enter runs`(MUTED), 다음 줄 `$`(MUTED) 1칸 접속 명령(ACCENT, 값 열 폭에서 최대 3줄 줄바꿈). 필터 입력 중에는 이 두 줄을 그리지 않는다(이때 `enter`는 필터 적용이다).
  - 적용값 조회 실패: 라벨 줄 대신 `Effective settings: <오류>`(ERROR, 줄바꿈)
- 키 안내:
  - 평소: ` enter ` connect, `n` new, `e` edit, `d` delete, `/` filter, `r` reload, `q` quit
  - 필터 입력 중: ` enter ` apply filter, `↑↓` select, `esc` clear filter
  - 호스트가 없어 접속할 수 없으면 배지를 비활성으로

## 2. 접속 폼 (`NewConnection.dc.html`, `EditError.dc.html`)

```
sshtui  New connection
────────────────────────────────────────────────────────────────────────────────────────────────

  Nickname    prod-web-3                            optional · fill in to save this host

  User        deploy                                optional

  Address     203.0.113.12                          required

  Port                                              optional

› Key file    ~/.ssh/id_ed25519                     optional

────────────────────────────────────────────────────────────────────────────────────────────────
On enter
  1 Save  prod-web-3 to ~/.ssh/config
  2 Run   $ ssh prod-web-3

Leave Nickname empty to connect once without saving.
────────────────────────────────────────────────────────────────────────────────────────────────
 enter  save & connect   tab next   ctrl+l pick key   ctrl+o browse   esc back
```

- 머리줄 제목: `New connection` 또는 `Edit <alias>`(TEXT 굵게). 편집이면 오른쪽에 `<출처 경로>:<줄>`(MUTED, `~` 축약).
- 1행 구분선, 2행 빈 줄. 필드는 한 줄씩, 필드 사이에 빈 줄 하나.
- 필드 줄: 표시 칸 2, 라벨 칸 11, 입력칸(폭 36. 화면이 좁으면 줄여서 최소 10), 2칸, 안내.
  - 포커스 없는 필드: 라벨 MUTED, 입력칸 배경 INPUT.
  - 포커스 필드: 표시 칸에 `›`, 라벨과 `›`는 ACCENT 굵게, 입력칸 배경 INPUT_FOCUS. 커서는 `frame.set_cursor_position`으로 입력 위치에 둔다.
  - 입력칸 안쪽 좌우 1칸은 여백이고 값은 그 사이에 그린다. 값이 길면 커서가 보이도록 가로 스크롤(기존 동작). 클릭 영역의 `column` 계산도 이 여백을 반영한다.
  - 값이 비어 있고 placeholder가 있으면 입력칸 안에 `inherited: <값>`(MUTED).
  - 읽기 전용 필드: 입력칸 배경 없이 원문 줄(MUTED, 여러 줄이면 아래로 이어서), 안내는 `read-only`(WARN).
  - 안내 문구:
    - 새 접속: Nickname은 `optional · fill in to save this host`(MUTED), Address는 `required`(WARN), 나머지는 `optional`(MUTED)
    - 편집: Nickname은 `required`(WARN), Address는 블록에 HostName이 원래 있으면 `required`(WARN) 없으면 `optional`, 나머지는 `optional`
    - 폭이 부족하면 안내를 자르거나 생략한다.
  - 검증 오류(값이 비어 있지 않은 필드만): 그 필드의 `›`와 라벨을 ERROR로(포커스면 굵게) 그리고, 필드 바로 다음 줄에 입력칸 시작 열(13열)에 맞춰 오류 문구(ERROR)를 그린다. 그 뒤에 빈 줄이 온다. 문구는 기존 검증 메시지를 쓰되 첫 글자만 대문자로 바꾼다.
- 필드 뒤 구분선, 이어서 `On enter` 영역:
  - 첫 줄 `On enter`(MUTED). 단계 줄은 2칸 들여쓰기, 번호(MUTED), 1칸, 동사(폭 5), 1칸, 내용.
  - 새 접속, 검증 통과, Nickname 있음: `1 Save  <nickname>(굵게) to <config 경로, ~ 축약>` 과 `2 Run   $(MUTED) <명령>(ACCENT)`. 빈 줄 뒤 `Leave Nickname empty to connect once without saving.`(MUTED)
  - 새 접속, 검증 통과, Nickname 없음: `1 Run   $ <명령>`. 빈 줄 뒤 `Fill in Nickname to save this host to <config 경로>.`(MUTED)
  - 편집, 검증 통과: `1 Review changes to <출처 경로, ~ 축약>`
  - 검증 오류: 단계 대신 2칸 들여쓰기로 ERROR 한 줄. 값이 빈 필수 필드면 `Fill in <라벨> to continue`, 그 외에는 `Fix <라벨> to continue`. 라벨은 첫 오류 필드의 화면 라벨(`Nickname`, `User`, `Address`, `Port`, `Key file`).
  - 공유 알림(`shared_notice`)이 있으면 빈 줄 뒤 WARN으로 줄바꿈해 표시.
  - 찾아보기 비활성 사유가 있으면 MUTED 한 줄로, `hidden_files_hint`가 있으면 MUTED 한 줄로 이어서 표시(Key file에 포커스가 있을 때만).
  - 명령과 긴 줄은 폭에 맞춰 줄바꿈하고, 높이가 부족하면 아래 줄부터 생략한다. 필드와 커서가 우선이다.
- 키 안내:
  - 주 동작: 새 접속은 ` enter ` `save & connect`(Nickname 있음) 또는 `connect`(없음), 편집은 `review changes`. 검증 오류가 있으면 비활성 배지.
  - `tab` next
  - Key file에 포커스가 있으면 `ctrl+l` pick key, `ctrl+o` browse(비활성 사유가 있으면 키도 MUTED). 다른 필드면 `ctrl+u` clear field
  - `esc` back

## 3. 변경 확인 (`ReviewChanges.dc.html`)

```
sshtui  Save staging                                                               ~/.ssh/config
────────────────────────────────────────────────────────────────────────────────────────────────
2 lines will change  -2  +2

   21   21      Host staging
   22   22          HostName staging.example.com
   23        -      Port 22
        23   +      Port 2222
   25   25
The current file is kept as ~/.ssh/.config.sshtui.bak
────────────────────────────────────────────────────────────────────────────────────────────────
 y  save   n back to form   ↑↓ scroll
```

- 머리줄 제목 `Save <alias>` 또는 `Delete <alias>`(TEXT 굵게), 오른쪽에 대상 파일 경로(MUTED, `~` 축약, 길면 앞을 잘라 한 줄).
- 1행 구분선. 2행 요약: `N lines will change`(N은 삭제 줄 수와 추가 줄 수 중 큰 값, 1이면 `1 line will change`), 2칸, 삭제가 있으면 `-<삭제 수>`(ERROR), 2칸, 추가가 있으면 `+<추가 수>`(OK). 3행 빈 줄.
- diff 줄: 이전 줄 번호(폭 5 오른쪽 정렬), 새 줄 번호(폭 5 오른쪽 정렬), 기호 칸(폭 4, 기호는 셋째 칸, 굵게), 내용.
  - 문맥 줄: 전부 MUTED
  - 삭제 줄: 줄 폭 전체 배경 DEL_BG, 글자 ERROR
  - 추가 줄: 줄 폭 전체 배경 ADD_BG, 글자 OK
  - hunk 사이에는 `⋯`(MUTED) 한 줄
  - 스크롤 위치 반영(기존 동작)
- diff 아래(아래 구분선 위, 상태 메시지가 있으면 그 위): `The current file is kept as <백업 경로>`(MUTED). 백업 경로는 대상 경로의 디렉터리에 `config::writer::backup_file_name`을 붙여 만든다(`~` 축약). 대상 파일이 아직 없는 경우(새로 만드는 경우)에는 이 줄을 그리지 않는다.
- 키 안내: 편집은 ` y ` save, `n` back to form, `↑↓` scroll. 삭제는 ` y ` delete, `n` back to list, `↑↓` scroll.

## 4. 팝업 (`KeyPicker.dc.html`)

팝업이 뜨면 뒤 화면(폼 또는 목록)을 `theme.dimmed()`로 그린다: 모든 글자 MUTED(제품명과 선택 표시 포함), 입력칸 배경 INPUT_DIM, 선택 줄 배경 없음, 안내 문구와 `On enter` 영역과 상세의 `enter runs`는 그리지 않는다. 머리줄과 아래 구분선은 남고 키 안내는 팝업의 것으로 바뀐다.

팝업 상자: 가로 가운데, 폭은 60칸(화면이 좁으면 `w - 4`), 위쪽은 7행(들어가지 않으면 세로 가운데). 테두리는 직각 선(`BorderType::Plain`)을 TEXT 색으로, 안쪽 배경 BG, 안쪽 좌우 여백 2칸. 상자 아래는 `Clear`로 지운다.

키 목록:

```
┌──────────────────────────────────────────────────────────┐
│  Select a key file                    4 keys in ~/.ssh   │
│                                                          │
│  › ~/.ssh/id_ed25519                ssh-ed25519 me@host  │
│    ~/.ssh/id_rsa                                         │
└──────────────────────────────────────────────────────────┘
```

- 제목 줄: `Select a key file`(TEXT 굵게), 오른쪽 `<N> keys in <ssh 디렉터리, ~ 축약>`(MUTED. 1개면 `1 key in ...`). 빈 줄.
- 키 줄: 표시 칸 2, 경로(`paths.abbreviate_home(&key.path)`), `.pub` 정보가 있으면 오른쪽 끝에 `<종류> <코멘트>`(MUTED, 폭이 부족하면 자름). 선택 줄은 배경 ROW, `›` ACCENT 굵게, 경로 굵게. 선택 줄이 보이도록 스크롤.
- 키 안내: ` enter ` use this key, `↑↓` select, `esc` cancel

알림(목업에 없음, 같은 규칙 적용): 제목 `Saved settings differ from effective SSH settings`(굵게), 빈 줄, 본문 줄(WARN, 줄바꿈). 키 안내는 접속으로 이어지면 ` enter ` connect anyway, `esc` back to list. 아니면 ` enter ` ok.

Update(목업에 없음, 같은 규칙 적용): 제목 줄 `Update sshtui`(굵게), 오른쪽 `<현재> → <최신>`(MUTED). 빈 줄. 라벨 칸(폭 15, MUTED)과 값: `Installed via`, 이어서 실행 가능하면 `Command` 줄들(각 줄 `$`(MUTED) 1칸 명령(ACCENT), 줄바꿈), 불가능하면 `Download`와 릴리스 URL. 키 안내는 실행 가능하면 ` enter ` update and exit, `esc` cancel. 아니면 ` esc ` close(배지).

## App에 추가할 정보 (src/app, 순수)

- `HostRow`에 `summary: &str` 추가: 그 항목의 블록에 직접 적힌 값으로 만든 `user@hostname`. `block_fields` 결과에서 `User`와 `HostName`이 `FieldState::Value`일 때만 사용한다. HostName이 없으면 별칭을 주소 자리에 쓰고, User가 없으면 `@` 없이 주소만. 다시 불러올 때 한 번 계산해 보관한다(`ssh -G`를 호스트마다 실행하지 않는다).
- `pub fn selected_command(&self) -> Option<String>`: 선택된 호스트의 접속 명령 미리보기(`preview(connect_args(.., Alias))`, prefix는 `loaded_form_preview`와 같은 방식으로 불러온 설정 기준). 선택 항목이 없으면 `None`.
- `pub fn form_first_error(&self) -> Option<(FormField, ValidationError)>`: 현재 폼의 첫 검증 오류.
- 그 밖에 렌더러가 필요한 읽기 전용 접근자(편집 대상의 출처 경로와 줄 번호 등).

## 마우스 (HitMap)

- 유지: 호스트 줄(`HostRow`), 폼 필드 줄(`FormField`, 표시 칸과 라벨과 안내 칸은 `column == u16::MAX`), 키 목록 줄(`KeyRow`), 팝업 `Outside`와 `Inert`, 머리줄 업데이트 알림(`UpdateNotice`).
- 키 안내: 쌍마다 `Key(code)`. 대응: `enter`는 `Enter`, `esc`는 `Esc`, `tab`은 `Tab`, `ctrl+l`은 `F(2)`, `ctrl+o`는 `F(3)`, 한 글자 키는 `Char`. `ctrl+u`와 `↑↓`는 등록하지 않는다.
- 추가: 목록의 필터 바 전체는 `Key(Char('/'))`(입력 중이 아닐 때).
- 버튼 영역은 없어진다.

## 작은 화면

어떤 크기(1x1 포함)에서도 패닉이 없어야 한다. 폭이나 높이가 부족하면 뒤쪽 요소부터 생략한다. 영역 계산은 `saturating_sub`를 쓴다.

## 테스트

`TestBackend`로 그린다. 색 검사는 `Theme::new(ColorMode::TrueColor)` 기준이고, Ansi 모드는 별도 테스트로 확인한다. 목업과 같은 크기(96x26에 여백을 더한 100x28)와 목업과 같은 표본 데이터(`Main.dc.html`의 호스트 12개 가운데 일부)를 쓰는 테스트를 화면마다 하나 이상 둔다.

1. 목록: 머리줄(`sshtui` ACCENT 굵게, `N hosts` MUTED), 필터 바 배경 INPUT과 placeholder, 위아래 구분선 문자와 RULE 색
2. 목록: 줄의 열 정렬(표시 칸, 이름 칸, 요약), 선택 줄의 `›`와 배경 ROW와 굵은 이름, 다른 줄의 요약 MUTED
3. 목록: 읽기 전용 줄 오른쪽 끝의 `read-only`(WARN)
4. 목록: 세로 구분선 `│`와 상세(별칭 굵게, `user@address` MUTED, 라벨 5종의 값 시작 열 일치, `enter runs`, `$ ssh <alias>` ACCENT)
5. 목록: 필터 입력 중 필터 바 배경 INPUT_FOCUS, `/` ACCENT, 일치 부분 ACCENT, 제목 `3 of 12 hosts`, 키 안내 3쌍, 상세에 `enter runs` 없음, 커서 위치
6. 목록: 폭 63에서는 상세 열과 `│`가 없고 폭 64에서는 있음. 폭 64 미만에서는 좌우 여백 없음
7. 목록: 업데이트 알림(`update available` WARN, `u` 굵게)과 계획이 없을 때 없음
8. 키 안내: 배지 셀(배경 ACCENT, 글자 BG, 굵게), 다른 키 TEXT 굵게, 설명 MUTED, 쌍 사이 3칸
9. 폼(새 접속): 필드 5개의 행 간격(필드 사이 빈 줄 하나), 라벨 열과 입력칸 열과 안내 열, 입력칸 배경 INPUT, 포커스 필드의 `›`와 ACCENT 굵은 라벨과 INPUT_FOCUS, 커서 위치(한글 값 포함)
10. 폼: 안내 문구(새 접속과 편집 각각, `required`는 WARN)
11. 폼: `On enter` 영역 4가지(Nickname 있음, 없음, 편집, 검증 오류)와 키 안내의 주 동작 문구
12. 폼: 검증 오류 시 라벨 ERROR, 바로 다음 줄의 오류 문구와 시작 열, 비활성 배지(배경 ROW, 글자 MUTED), `Fix Port to continue`
13. 폼: 값이 빈 Address는 오류 줄이 없고 `On enter`에 `Fill in Address to continue`
14. 폼: 포커스에 따른 키 안내(Key file이면 `ctrl+l`과 `ctrl+o`, 아니면 `ctrl+u`)
15. 폼: placeholder `inherited: ...`(MUTED), 읽기 전용 필드의 원문과 `read-only`, 공유 알림, 찾아보기 비활성 사유
16. 확인: 머리줄 제목과 오른쪽 경로, 요약 줄(`2 lines will change`, `-2` ERROR, `+2` OK, 단수형), 줄 번호 열과 기호 열, 삭제 줄 배경 DEL_BG와 추가 줄 배경 ADD_BG가 줄 폭 전체, 문맥 줄 MUTED, 백업 안내 줄, 키 안내(편집과 삭제)
17. 키 목록: 뒤 화면이 dimmed(제품명 MUTED, 입력칸 배경 INPUT_DIM, 안내 없음), 팝업 테두리 문자(`┌`, `┐`, `└`, `┘`)와 TEXT 색, 제목과 `N keys in ~/.ssh`, 선택 줄 배경 ROW, `.pub` 정보 MUTED, 키 안내
18. 알림과 Update 팝업의 내용과 키 안내
19. 상태 메시지 위치(아래 구분선 바로 위)와 색 3종, 3줄 제한
20. TrueColor 모드에서 빈 칸의 배경이 BG, Ansi 모드에서는 Reset이고 입력칸에 밑줄
21. `HitMap`: 필터 바, 호스트 줄, 폼 필드(`column` 여백 반영), 키 안내 쌍(`ctrl+l`은 `F(2)`, `ctrl+u`는 None), 팝업 3종 영역, 업데이트 알림
22. 모든 화면 종류가 1x1, 5x2, 20x5, 40x10, 63x17, 64x18, 100x28, 200x60에서 패닉 없음
23. app: `HostRow::summary` 4가지 경우(User와 HostName 모두 있음, HostName만, User만, 둘 다 없음), `selected_command`, `form_first_error`
24. main: `color_mode`(None, `truecolor`, `24bit`, `TrueColor`, 다른 값)

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- `cargo check --offline --target x86_64-pc-windows-msvc --all-targets`와 `cargo check --offline --target x86_64-unknown-linux-gnu --all-targets` 통과
- 전체 테스트 수가 362개 이상
- `src/ui/` 밖의 기존 테스트 단언은 변경 없음(`ui::render` 호출부의 인자 추가는 허용)
- `docs/design/` 변경 없음
