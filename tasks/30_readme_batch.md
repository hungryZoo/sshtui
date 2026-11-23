# 작업 30: README에 작업 24-29 반영

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `tasks/11_readme.md`의 "작성 원칙"과 "구조 (위키형)"을 그대로 적용한다(한국어, 명사형 개조식, 비유 금지, 원 숫자 기호 금지, 범위는 `-`, 목차 앵커 일치). 가운뎃점은 화면 문구를 인용하는 코드 표기 안에서만 허용한다.

## 목표

작업 24-29의 변경을 `README.md`에 반영한다. 코드는 수정하지 않는다. 모든 내용은 소스(`src/history.rs`, `src/state.rs`, `src/i18n.rs`, `src/ssh/keysetup.rs`, `src/ssh/exec.rs`, `src/app/`, `src/ui/`, `src/tui.rs`, `src/main.rs`, `src/system.rs`)와 대조해 확인한다. 화면 문구 인용은 영어 표시 기준으로 쓰고, 한국어 표시는 `표시 언어` 절에서 설명한다.

## 대상 파일

- `README.md`

## 변경 내용

1. `1. 개요`의 `기능`에 추가: 마지막 접속 호스트에서 시작, 셸 히스토리의 ssh 접속 후보 가져오기, 폼의 키 목록 상시 표시, 서버에 공개 키 설치(키 로그인 설정), 비밀번호 입력 끄기 옵션, 한국어와 영어 표시. `범위 외 기능`에 키 쌍 생성(`ssh-keygen`)과 서버의 sshd 설정 변경이 없으면 추가.
2. `3. 사용법`의 `호스트 목록`:
   - 시작 커서: 마지막으로 접속한 저장 호스트. 기록 파일 위치(`src/state.rs`의 경로 규칙을 OS별 표로: Linux는 `$XDG_STATE_HOME` 또는 `~/.local/state`, macOS는 `~/Library/Application Support`, Windows는 `%LOCALAPPDATA%` 아래 `sshtui/last-host`. `dirs` 크레이트의 실제 규칙과 대조), 기록 시점(저장된 별칭으로 접속할 때), 1회 접속은 기록하지 않음
   - 맨 아래 `+ New connection` 줄: 선택과 `Enter`, 한 번 클릭, 필터 결과 0건일 때의 표시, `End`와 `G`는 마지막 호스트
   - 키 표와 키 안내 갱신: `n new` 안내 제거(키는 동작), `x forget(known_host)`, `i`(후보 팝업), new 줄 선택 시 `enter new`
   - 머리줄의 `N suggestions i` 알림과 업데이트 알림과의 배치
   - 저장된 호스트가 없어도 히스토리 후보가 있으면 목록에서 시작한다는 점
3. `3. 사용법`의 `접속 폼`:
   - 필드 7개와 포커스 순서, 체크 항목 2개(`Key login`, `Password`)의 문구와 `Space` 전환
   - `Key file` 아래 키 목록: 표시 조건, 줄 구성(`›`, `●`), 최대 4줄과 `… N more`, `↑↓` 탐색 규칙(입력칸과 키 줄 사이 이동, 마지막 키 다음은 다음 필드), 클릭, 세로 공간이 부족할 때의 배치 우선순위
   - 키 안내 표 갱신(`↑↓ pick key`, `space toggle`). `ctrl+l`과 `F2` 팝업은 안내에 없지만 계속 동작
   - `On enter` 단계에 `Key   install ... on the server`가 추가되는 경우
4. 새 하위 절 `키 로그인 설정`(`접속 폼` 다음, 목차 포함):
   - 하는 일: 선택한 키 파일의 `.pub` 한 줄을 시스템 `ssh`의 표준 입력으로 보내 서버의 `~/.ssh/authorized_keys`에 추가(`ssh-copy-id`와 같은 목적, 같은 명령을 쓰지는 않음). 원격에서 실행되는 스크립트의 동작을 항목으로 요약(디렉터리와 파일 권한, 중복 방지, 줄 끝 보정, `restorecon`)
   - 실행 방식: TUI를 잠시 내려놓고 ssh 실행, 서버 비밀번호 입력, 결과 확인 후 `Enter`로 복귀. 설치용 호출에는 `-o PasswordAuthentication=yes`가 붙어 비밀번호 입력을 꺼 둔 호스트에서도 설치 가능
   - 진행 순서 표: 새 접속(닉네임 없음), 새 접속(닉네임 있음), 편집(변경 있음), 편집(변경 없음) 각각의 저장, 설치, 접속 순서와 실패 시 결과
   - 오류와 메시지: `Key login needs a key file`, 공개 키 파일 없음, 비밀키 파일 거부, `Key install failed for ...`
   - `Password` 체크 항목: config에 `PasswordAuthentication no`를 쓰거나 지움, 1회 접속에서는 `-o PasswordAuthentication=no`. 이 클라이언트의 해당 호스트 접속에만 적용되고 서버의 sshd 설정은 바꾸지 않음. keyboard-interactive 방식의 비밀번호 질문은 이 옵션으로 꺼지지 않음. 키 로그인이 되기 전에 켜면 접속할 수 없으므로 화면에 경고 문구가 표시됨
   - 제한: `.pub` 파일이 있어야 함(키 쌍 생성 기능 없음), 서버에 POSIX `sh` 필요, Windows 서버 미지원
5. `변경 확인` 절: 삭제와 호스트 키 삭제 확인의 질문 줄, 두 선택지, 기본 선택 Cancel, 키(`←→`, `Tab`, `h`/`l`, `Enter`, `y`, `n`, `Esc`), 클릭, 키 안내 문자열. 저장 확인은 기존과 같음.
6. 새 하위 절 `셸 히스토리 가져오기`(`호스트 키 삭제` 앞 또는 뒤, 목차 포함):
   - 읽는 파일 표(순서대로: `HISTFILE`, `$ZDOTDIR/.zsh_history`, `~/.zsh_history`, `~/.bash_history`, fish, PowerShell)와 형식 판별 규칙
   - 읽는 방식: 읽기 전용, 파일 끝 256KiB, `ssh`로 시작하는 명령만 모듈 밖으로 전달, 화면에는 추출한 접속 값만 표시
   - 해석 규칙 요약 표: 사용하는 옵션(`-p`, `-l`, `-i`, `-o Port/User/IdentityFile/HostName`), 건너뛰는 명령(셸 특수 문자, `sudo ssh`, `-J`, `-F`, `ProxyJump` 등), 사용자와 포트의 우선순위
   - 후보 규칙: 합치는 기준, 최신순, 최대 20개, 저장된 별칭이나 `HostName`과 같은 주소 제외
   - 흐름: 머리줄 알림 또는 `i` → 팝업 → `Enter`로 값이 채워진 새 접속 폼 → 닉네임을 붙여 저장하면 후보에서 사라짐
   - 제한: 셸이 아직 파일에 쓰지 않은 현재 세션의 명령은 보이지 않음, sshtui로 한 접속은 셸 히스토리에 남지 않음, 거절한 후보를 기억하지 않음
7. 새 하위 절 `표시 언어`(`색` 앞, 목차 포함): 결정 순서(`SSHTUI_LANG`, `LC_ALL`, `LC_MESSAGES`, `LANG`, 시스템 로캘), `C`와 `POSIX`의 처리, 한국어로 판정되는 값의 예, 번역 범위와 번역하지 않는 것(키 이름, 하위 계층 오류, `--help` 등), 한글 2칸 폭 처리.
8. `환경변수` 표: `SSHTUI_LANG`, `SSHTUI_NO_HISTORY`, `SSHTUI_HISTORY_FILE` 추가. `--help` 출력과 일치 확인.
9. `마우스` 절: new 줄 클릭, 폼의 키 줄과 체크 항목 클릭, 확인 화면의 선택지 클릭, 후보 알림과 후보 줄 클릭 반영.
10. `9. 개발`의 `모듈 구성` 표: `src/history.rs`, `src/state.rs`, `src/i18n.rs`, `src/ssh/keysetup.rs`, `src/app/import.rs` 반영. 의존성 `sys-locale` 한 줄.
11. `검증 상태`와 `수동 확인 절차`: 이 파일 끝의 "검수자 확인 내용"을 그대로 반영한다. 단위 테스트 수는 `cargo test --offline`의 실제 결과.

## 검수자 확인 내용

검수자가 macOS의 tmux 창에서 임시 HOME과 임시 SSH 디렉터리로 직접 확인한 내용이다. `검증 상태` 표의 수동 행은 아래 항목을 주제별 행으로 나눠 적고 상태는 `확인 (macOS, tmux)`로 쓴다. Codex가 스스로 수행한 확인은 수동 행으로 적지 않는다.

- 목록: 저장된 호스트로 접속한 뒤 다시 실행하면 그 호스트에 커서(임시 HOME의 `Library/Application Support/sshtui/last-host` 생성 확인), `+ New connection` 줄 선택과 상세 열 문구, 키 안내 `x forget(known_host)`, 삭제 확인에서 `Enter`만 누르면 취소되고 config 불변, `→` 후 `Enter`로 삭제, 강조된 `Delete`의 오류 색 배지
- 접속 폼(100x28, 80x24): `Key file` 아래 키 줄 2개와 종류, 코멘트 표시, `↓`로 키 선택 시 값 변경과 `●` 표시, 체크 항목의 `Space` 전환과 경고 문구, 80x24에서 빈 줄 없는 배치로 필드, 키 줄, 체크 항목, `On enter` 영역 모두 표시
- 키 로그인 설정: `PATH` 앞에 둔 가짜 `ssh`(설치 호출이면 원격 스크립트를 임시 원격 홈에서 실행)로 새 호스트 저장 → 설치 → 접속 순서 확인. 표준 입력이 `.pub` 파일과 바이트 일치, 임시 원격 홈의 `.ssh` 0700과 `authorized_keys` 0600, 설치 인자의 `-o PasswordAuthentication=yes`, config의 `PasswordAuthentication no` 기록, 편집 폼에서 설치 실패(종료 코드 255) 시 목록 복귀와 `Key install failed for kl (ssh exit code 255)`, 편집에서 `Password` 해제 시 해당 줄 삭제 diff
- 셸 히스토리 가져오기: `SSHTUI_HISTORY_FILE`로 준 zsh 확장 형식 표본에서 머리줄 `3 suggestions i`, 팝업의 최신순 정렬과 같은 접속의 `×2` 합산, 저장된 `HostName`과 같은 주소 제외, `-J` 명령 제외, 후보 선택 시 4개 필드가 채워진 새 접속 폼
- 표시 언어: `LANG` 미설정이고 시스템 로캘이 `ko-KR`일 때 한국어, `LANG=en_US.UTF-8`이면 영어, `SSHTUI_LANG=ko`가 `LANG`보다 우선. 한국어 목록, 폼, 키 목록 팝업, 히스토리 팝업, 삭제 확인 화면에서 표시 폭을 계산해 세로 구분선과 팝업 테두리의 열 일치, 화면 폭 초과 없음(100x28, 96x26, 80x24). 폭 96의 필터 적용 목록에서 키 안내가 `esc 필터 해제`까지 표시

`검증 상태` 표에 추가할 미검증 행:

- 실제 서버에 대한 공개 키 설치와 그 뒤의 키 로그인(가짜 `ssh`와 임시 홈에서의 스크립트 실행까지만 확인)
- 실제 사용자의 셸 히스토리 파일. bash, fish, PowerShell 형식은 단위 테스트로만 확인
- Windows와 Linux에서의 언어 판별, 상태 파일 경로, 히스토리 파일 경로
- 실제 터미널 앱과 글꼴에서의 한글 2칸 폭 렌더링

`수동 확인 절차`에는 위 5개 주제를 임시 디렉터리에서 재현하는 단계를 추가한다. 키 로그인 단계는 실제 서버가 필요하다는 점과, 테스트 서버의 `authorized_keys`가 바뀐다는 점을 적는다. 모든 명령은 임시 `HOME`과 `SSHTUI_SSH_DIR`을 쓰고 실제 `~/.ssh`와 실제 히스토리를 건드리지 않아야 한다(히스토리는 `SSHTUI_HISTORY_FILE`로 표본 파일 지정).

## 완료 조건

- 목차의 앵커 링크가 모두 실제 제목과 일치
- 원 숫자 기호 없음. 가운뎃점은 코드 표기 안의 화면 문구 인용에만 있음
- README에 `PTY` 문자열 없음(수동 확인 행의 환경 표기는 검수자 확인 내용의 표기를 따름)
- `git status`에 `README.md`만 변경으로 표시
