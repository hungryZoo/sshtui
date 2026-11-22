<div align="center">

# `sshtui`

**터미널에서 SSH 호스트를 관리하고 접속하는 TUI**

OpenSSH 설정 파일과 시스템 `ssh`를 사용하는 Rust CLI

![built with Rust and ratatui](https://img.shields.io/badge/built%20with-Rust%20%2B%20ratatui-orange?logo=rust)

```bash
cargo install --path .
```

![macOS](https://img.shields.io/badge/platform-macOS-lightgrey)
![Linux](https://img.shields.io/badge/platform-Linux-lightgrey)
![Windows](https://img.shields.io/badge/platform-Windows-lightgrey)

</div>

---

`sshtui`를 실행하면 SSH 설정에서 읽은 호스트 목록 표시.  
호스트가 없으면 `New connection` 폼으로 시작.  
저장된 호스트는 목록에서 선택하여 바로 접속.  
새 접속은 폼에서 1회 접속하거나 닉네임을 붙여 저장 후 접속.  
접속 시 TUI를 닫고 복원된 터미널에서 시스템 `ssh` 실행.

---

## Table of Contents

1. [개요](#1-개요)
   - [기능](#기능)
   - [동작 방식](#동작-방식)
   - [범위 외 기능](#범위-외-기능)
2. [설치](#2-설치)
   - [요구 사항](#요구-사항)
   - [소스 설치](#소스-설치)
3. [사용법](#3-사용법)
   - [실행](#실행)
   - [공통 화면 구성](#공통-화면-구성)
   - [호스트 목록](#호스트-목록)
   - [접속 폼](#접속-폼)
   - [변경 확인](#변경-확인)
   - [키 목록](#키-목록)
   - [불일치 알림](#불일치-알림)
   - [마우스](#마우스)
   - [접속 실행](#접속-실행)
   - [입력 검증](#입력-검증)
   - [색](#색)
   - [환경변수](#환경변수)
4. [ssh config 연동](#4-ssh-config-연동)
   - [읽기](#읽기)
   - [새 호스트 저장](#새-호스트-저장)
   - [편집과 삭제](#편집과-삭제)
   - [쓰기 공통 규칙](#쓰기-공통-규칙)
   - [닉네임 규칙](#닉네임-규칙)
   - [알려진 제한](#알려진-제한)
5. [업데이트](#5-업데이트)
   - [확인 방식](#확인-방식)
   - [캐시 파일](#캐시-파일)
   - [적용 흐름](#적용-흐름)
   - [설치 방식별 명령](#설치-방식별-명령)
   - [전제 조건](#전제-조건)
6. [네이티브 파일 다이얼로그](#6-네이티브-파일-다이얼로그)
   - [플랫폼별 동작](#플랫폼별-동작)
   - [공통 동작](#공통-동작)
   - [비활성 조건](#비활성-조건)
7. [검증 상태](#7-검증-상태)
   - [검증 결과](#검증-결과)
8. [수동 확인 절차](#8-수동-확인-절차)
   - [임시 SSH 디렉터리 확인](#임시-ssh-디렉터리-확인)
   - [마우스 확인](#마우스-확인)
   - [업데이트 확인](#업데이트-확인)
9. [개발](#9-개발)
   - [모듈 구성](#모듈-구성)
   - [검증 명령](#검증-명령)
   - [작업 방식](#작업-방식)

## 1. 개요

### 기능

- 저장된 호스트 목록에서 접속.
- 접속 폼을 통한 새 접속과 닉네임을 붙인 호스트의 `~/.ssh/config` 저장.
- 저장된 호스트 편집과 삭제.
- 키 목록, 네이티브 파일 다이얼로그, 직접 입력을 통한 키 파일 지정.

### 동작 방식

| 항목 | 방식 |
|---|---|
| 저장소 | 기본 `~/.ssh/config`, 별도 DB 없음 |
| 접속 | 시스템 `ssh` 실행, SSH 프로토콜 미구현 |
| 적용값 확인 | 시스템 `ssh -G` 실행 |
| 터미널에서 재사용 | 기본 설정에 저장한 호스트는 `ssh <닉네임>`으로 접속 |

### 범위 외 기능

- 포트 포워딩과 `ProxyJump` 등 고급 옵션 편집 UI.
- 그룹과 태그, 접속 기록, `scp`와 `sftp`, 비밀번호 저장.

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 2. 설치

### 요구 사항

- Rust stable 툴체인.
- `PATH`에서 실행할 수 있는 OpenSSH 클라이언트(`ssh`).
- TUI 입출력을 위한 대화형 터미널.

### 소스 설치

저장소 루트에서 설치.

```bash
cargo install --path .
```

또는 저장소 루트에서 빌드 후 실행.

```bash
cargo build --release
target/release/sshtui
```

Windows 빌드 실행 파일은 `target/release/sshtui.exe`. crates.io 배포 없음(`publish = false`).

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 3. 사용법

### 실행

```bash
sshtui
```

| 옵션 | 동작 |
|---|---|
| `-h`, `--help` | 도움말 출력 |
| `-V`, `--version` | 버전 출력 |
| `--check-update` | 즉시 새 버전 확인. 종료 코드 `0` 최신, `10` 업데이트 있음, `11` 접속 불가 |
| `--update` | 즉시 확인 후 새 버전이 있으면 설치. 종료 코드 `0` 완료 또는 최신, `1` 실패 또는 설치 방식 불명, `11` 접속 불가 |

모든 화면에서 `Ctrl+C`로 종료.

### 공통 화면 구성

- 머리줄 왼쪽에 `sshtui`와 화면 제목, 오른쪽에 화면별 보조 정보 표시. 목록은 업데이트 알림, 편집은 출처 파일과 줄 번호, 변경 확인은 대상 파일 경로 표시.
- 화면 공간에 따라 본문 상단과 하단에 가로 구분선 표시. 맨 아래에 키와 동작 안내, 첫 쌍의 키는 배지로 표시.
- 상태 메시지는 본문 아래, 하단 구분선과 키 안내 위에 최대 3줄 표시.

### 호스트 목록

시작 화면. 목록에 표시할 호스트가 없으면 `New connection`으로 시작.

- 머리줄에 전체 수(예: `4 hosts`) 표시. 필터 입력 중이거나 필터가 적용된 상태에서는 결과 수와 전체 수(예: `1 of 4 hosts`) 표시.
- 빈 필터 바에 `/ filter hosts` 표시. 필터 결과에서 이름과 일치하는 부분 강조, 결과가 없으면 `No matching hosts` 표시.
- 호스트 줄에 선택 표시 `›`, 이름, 블록의 직접 설정값으로 만든 `user@host` 요약 표시. `HostName` 값이 없으면 별칭 사용, `User` 값이 없으면 `user@` 생략. 읽기 전용 파일의 호스트는 공간이 충분할 때 줄 오른쪽에 `read-only` 표시.
- 오른쪽 상세 열에 별칭과 적용값의 `user@address`, `User`, `Address`, `Port`, `Key file`, `Source`(파일과 줄 번호) 표시. 적용값 조회 실패 시 `Effective settings:` 뒤에 오류 표시. 읽기 전용 파일은 `Read-only file` 표시.
- 필터 입력 중이 아니고 팝업이 닫혀 있을 때 상세 열의 `enter runs` 아래에 실행할 명령 표시. 터미널 폭 64칸 미만에서는 상세 열 생략.
- 캐시에 새 버전이 있고 표시 공간이 충분하면 머리줄 오른쪽에 `update available u` 표시.

| 화면 안내 | 실제 키 | 동작 |
|---|---|---|
| 없음 | `↑`, `k` / `↓`, `j` | 이전 / 다음 호스트 선택 |
| 없음 | `Home`, `g` / `End`, `G` | 첫 / 마지막 호스트 선택 |
| `enter connect` | `Enter` | 선택한 호스트로 접속 |
| `n new` / `e edit` / `d delete` | `n` / `e` / `d` | 새 접속 폼 / 선택 항목 편집 / 삭제 확인 |
| `/ filter` | `/` | 필터 입력 시작 |
| `↑↓ select` | `↑`, `↓` | 필터 입력 중 결과에서 선택 이동 |
| `enter apply filter` | `Enter` | 필터 입력을 끝내고 결과 유지 |
| `esc clear filter`(필터 입력 중) | `Esc` | 필터 초기화 |
| `update available u`(머리줄) | `u` | 새 버전이 있을 때 `Update sshtui` 팝업 열기 |
| `r reload` / `q quit` | `r` / `q` | 설정 다시 읽기 / 종료 |

### 접속 폼

- 필드 줄은 선택 표시 `›`, 라벨, 입력칸, 안내 문구 순서. 상속값은 빈 입력칸에 `inherited:`로 표시.
- 새 접속의 `Nickname` 안내는 `optional · fill in to save this host`. 그 외 필드 안내는 `required`, `optional` 또는 `read-only`.
- 새 접속의 빈 `Port` 입력칸에는 보조 표시 `22`. 값을 비워 두면 `-p` 생략.

| 필드 | 새 접속 | 편집 |
|---|---|---|
| `Nickname` | 선택 | 필수 |
| `User` | 선택 | 선택 |
| `Address` | 필수 | 기존 `HostName`이 있으면 필수. 읽기 전용 값은 원본 유지 |
| `Port` | 선택 | 선택 |
| `Key file` | 선택 | 선택 |

화면 공간이 충분하면 필드 아래 구분선 뒤에 `On enter` 영역 표시.

| 상태 | `On enter` 표시 |
|---|---|
| 새 접속, 닉네임 있음 | `Save <닉네임> to <경로>`와 `Run` 명령 미리보기. `Leave Nickname empty to connect once without saving.` 안내 |
| 새 접속, 닉네임 없음 | 1회 접속의 `Run` 명령 미리보기. `Fill in Nickname to save this host to <경로>.` 안내 |
| 편집 | `Review changes to <경로>` |
| 검증 오류 | 첫 오류에 대해 빈 필수값은 `Fill in <필드> to continue`, 잘못된 값은 `Fix <필드> to continue` |

- 비어 있지 않은 필드의 검증 오류는 해당 입력칸 바로 아래에 표시. 제출 시 오류는 하단 상태 메시지에도 표시.
- `Key file`에 포커스가 있을 때 `ctrl+l pick key`와 `ctrl+o browse` 표시. 다른 필드에서는 `ctrl+u clear field` 표시. 다이얼로그를 사용할 수 없으면 `ctrl+o browse` 흐리게 표시.

| 화면 안내 | 실제 키 | 동작 |
|---|---|---|
| `tab next` | `Tab`, `↓` / `Shift+Tab`, `↑` | 다음 / 이전 필드 |
| 없음 | `←`, `→`, `Home`, `End` | 필드 내 커서 이동 |
| 없음 | `Backspace`, `Delete` | 문자 삭제 |
| `ctrl+u clear field` | `Ctrl+U` | 편집 가능한 필드 비우기 |
| `enter connect` | `Enter` | 닉네임이 없는 새 접속을 저장 없이 실행 |
| `enter save & connect` | `Enter` | 새 호스트 저장 후 닉네임으로 접속. 적용값 불일치 시 먼저 알림 표시 |
| `enter review changes` | `Enter` | 편집 내용의 변경 확인 화면 열기. 변경이 없으면 `No changes`를 표시하고 목록으로 복귀 |
| `esc back` | `Esc` | 목록 복귀. 표시할 호스트가 없으면 종료 |
| `ctrl+l pick key` | `Ctrl+L`, `F2` | 어느 필드에서나 SSH 디렉터리의 비밀키 목록 열기. `F2`는 화면 표기 없음 |
| `ctrl+o browse` | `Ctrl+O`, `F3` | 어느 필드에서나 네이티브 파일 다이얼로그 요청. `F3`는 화면 표기 없음 |

- 1회 접속에서 빈 `User`는 주소의 `user@` 부분 생략, 빈 `Port`와 `Key file`은 각각 `-p`와 `-i` 생략. 새 호스트 저장 시 빈 필드에 해당하는 설정 옵션도 생략.
- 키 파일 지정 방법은 키 목록 선택, 파일 다이얼로그 선택, `Key file` 직접 입력. 직접 입력에 사용한 `~` 또는 `~/` 경로는 1회 접속 인자를 만들 때 홈 디렉터리로 확장. 저장 시에는 입력한 경로 표기를 설정 파일에 기록.

### 변경 확인

- 편집 제목은 `Save <닉네임>`, 삭제 제목은 `Delete <닉네임>`.
- 요약 줄에 삭제와 추가 중 큰 줄 수를 `N lines will change`로 표시. 한 줄이면 `1 line will change`. 삭제 수와 추가 수는 각각 `-N`, `+N`으로 표시하며 0인 항목은 생략.
- 본문의 왼쪽 두 열은 변경 전과 변경 후 줄 번호. 삭제 줄은 이전 번호와 `-`, 추가 줄은 이후 번호와 `+`, 문맥 줄은 양쪽 번호 표시. 떨어진 변경 구간 사이에 `⋯` 표시.
- 기존 파일이 있고 공간이 충분하면 본문 하단에 `The current file is kept as <백업 경로>` 표시.

| 화면 안내 | 실제 키 | 동작 |
|---|---|---|
| `y save` / `y delete` | `y`, `Enter` | 변경 사항 기록 |
| `n back to form` / `n back to list` | `n`, `Esc` | 취소 후 편집은 폼, 삭제는 목록으로 복귀 |
| `↑↓ scroll` | `↑`, `↓` / `PageUp`, `PageDown` | 줄 / 페이지 단위 스크롤 |

### 키 목록

- 키 목록, 불일치 알림, 업데이트는 뒤 화면을 흐리게 표시하고 직각 테두리 상자에 내용을 표시하는 팝업. 키 안내는 화면 맨 아래에 표시.
- `Select a key file` 제목 오른쪽에 키 개수와 SSH 디렉터리 표시. 비밀키 헤더로 판별한 파일의 경로와 선택 표시 `›`, 대응하는 `.pub` 파일이 있으면 종류와 코멘트 표시.

| 화면 안내 | 실제 키 | 동작 |
|---|---|---|
| `↑↓ select` | `↑`, `k` / `↓`, `j` | 이전 / 다음 키 선택 |
| `enter use this key` | `Enter` | `Key file`에 선택 경로 입력 후 폼으로 복귀 |
| `esc cancel` | `Esc` | 기존 값 유지 후 폼으로 복귀 |

### 불일치 알림

`Saved settings differ from effective SSH settings` 팝업. 저장 후 `ssh -G` 조회가 성공하고 폼에서 지정한 값과 적용값이 다를 때 차이 표시.

| 화면 안내 | 실제 키 | 동작 |
|---|---|---|
| `enter connect anyway` | `Enter` | 새 호스트 저장 뒤 닉네임으로 접속 |
| `enter ok` | `Enter` | 편집 저장 뒤 목록으로 복귀 |
| `esc back to list`(새 호스트 저장 뒤에만 표시) | `Esc` | 목록으로 복귀, 이미 저장한 변경 유지 |

### 마우스

| 위치 | 동작 |
|---|---|
| 필터 바 클릭 | 필터 입력 시작 |
| 호스트 줄 클릭 | 호스트 선택 |
| 호스트 줄 더블 클릭(같은 칸, 400ms 이내) | 선택한 호스트로 접속 |
| 휠(목록, 변경 확인, 키 목록) | 목록과 키 목록에서 선택 이동, 변경 확인에서 스크롤 |
| 맨 아래 키 안내 클릭 | 해당 키와 같은 동작. `ctrl+u`와 `↑↓`는 클릭 대상에서 제외 |
| 머리줄의 업데이트 알림 클릭 | `Update sshtui` 팝업 열기 |
| 폼 필드 클릭 | 포커스 이동과 커서 위치 지정. 읽기 전용 필드는 포커스만 이동 |
| 키 목록의 줄 클릭 | 키 선택과 확정 |
| 팝업 바깥 클릭 | 팝업 닫기(`Esc`와 동일) |

- 왼쪽 버튼을 뗄 때 클릭 처리. 누른 곳과 다른 대상 위에서 떼면 취소.
- 마우스 캡처 중에는 터미널 자체의 텍스트 선택 비활성화. 대부분의 터미널에서 `Shift`(macOS Terminal과 iTerm2는 `Option`)를 누른 채 끌어 선택 가능하며, 동작은 터미널마다 다름.
- `SSHTUI_NO_MOUSE=1`로 마우스 캡처 비활성화.
- 마우스 캡처가 켜진 상태에서 정상 종료할 때 캡처를 해제하고 남은 입력을 최대 250ms 동안 비운 뒤 터미널 복원. 남은 마우스 입력이 뒤따르는 `ssh`에 전달되는 것을 방지.

### 접속 실행

- TUI 종료와 터미널 복원 후 `ssh` 실행.
- Unix는 `exec`로 현재 프로세스 교체. Windows는 자식 프로세스로 실행하고 종료 코드 반환.
- 암호와 키 패스프레이즈 입력, 호스트 키 확인은 `ssh`가 담당.

### 입력 검증

| 필드 | 규칙 |
|---|---|
| `Nickname` | 입력 시 공백, 제어 문자, `*`, `?`, `!`, `#`, `"` 금지. 첫 문자 `-`와 `=` 금지. 대소문자를 구분한 정확히 같은 이름의 중복 금지 |
| `User` | 공백, 제어 문자, `#`, `"` 금지. 첫 문자 `-` 금지 |
| `Address` | 공백, 제어 문자, `@`, `#`, `"` 금지. 첫 문자 `-` 금지 |
| `Port` | 비어 있거나 ASCII 숫자로 구성된 1-65535 |
| `Key file` | 공백 허용. 제어 문자와 `"` 금지. 첫 문자 `-` 금지 |

### 색

- `COLORTERM`이 `truecolor` 또는 `24bit`이면 대소문자 구분 없이 목업의 24비트 팔레트 적용, 화면 전체 배경 채움.
- 그 외에는 터미널 기본 16색 사용, 화면 전체 배경은 터미널 기본값 유지, 입력칸은 밑줄 표시. 선택한 줄과 활성 배지에는 강조 배경 사용.

| 이름 | 값(RGB) | 용도 |
|---|---|---|
| `bg` | `(21, 23, 28)` | 화면 배경, 활성 배지의 글자 |
| `text` | `(216, 219, 225)` | 기본 글자, 팝업 테두리 |
| `muted` | `(141, 148, 160)` | 보조 글자, 팝업 뒤의 글자 |
| `accent` | `(111, 211, 193)` | 강조 글자, 활성 배지 배경 |
| `warn` | `(230, 181, 102)` | 경고, 읽기 전용과 업데이트 표시 |
| `error` | `(240, 138, 138)` | 오류, 삭제 줄 글자 |
| `ok` | `(156, 207, 130)` | 정상 상태 메시지, 추가 줄 글자 |
| `rule` | `(58, 64, 75)` | 가로 구분선과 열 구분선 |
| `input` | `(32, 36, 44)` | 일반 입력칸 배경 |
| `focus` | `(43, 49, 59)` | 포커스된 입력칸 배경 |
| `dim_input` | `(27, 30, 36)` | 팝업 뒤 입력칸 배경 |
| `row` | `(38, 43, 52)` | 선택 줄과 비활성 배지 배경 |
| `removed` | `(58, 34, 38)` | 삭제 줄 배경 |
| `added` | `(31, 51, 38)` | 추가 줄 배경 |

### 환경변수

| 이름 | 동작 |
|---|---|
| `SSHTUI_SSH_DIR` | SSH 디렉터리 변경. 미설정 또는 빈 값이면 `~/.ssh` 사용. 상대 경로는 현재 작업 디렉터리 기준 |
| `SSHTUI_NO_MOUSE` | 비어 있지 않고 `0`이 아닌 값이면 마우스 캡처 비활성화. 터미널 기본 텍스트 선택 유지 |
| `SSHTUI_NO_UPDATE_CHECK` | 비어 있지 않고 `0`이 아닌 값이면 자동 확인과 알림 비활성화. `--check-update`와 `--update`에는 영향 없음 |

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 4. ssh config 연동

### 읽기

- SSH 디렉터리의 `config`에서 `Include`를 재귀적으로 읽고 glob을 확장. 상대 경로는 SSH 디렉터리 기준, `~`와 `~/` 경로는 홈 디렉터리 기준.
- `Host`의 개별 패턴을 목록에 표시. `*` 또는 `?`가 포함된 패턴, `!`로 시작하는 패턴, `Match` 헤더는 목록에서 제외.
- 항목별 출처 파일과 `Host` 줄 번호 표시. 상세 열의 적용값은 `ssh -G <닉네임>` 결과 사용.

### 새 호스트 저장

- 루트 `config`가 없으면 생성. Unix에서 새 파일 권한은 `600`, 새 디렉터리 생성 모드는 `700`(umask 적용).
- 와일드카드 또는 부정 패턴이 있는 첫 `Host` 블록이나 첫 `Match` 블록 중 먼저 나오는 블록 앞에 삽입. 블록에 붙은 주석이 있으면 주석 앞에 삽입. 해당 블록이 없으면 파일 끝에 추가. SSH가 옵션별로 먼저 얻은 값을 적용하는 순서를 고려한 배치.
- `HostName`, `User`, `Port`, `IdentityFile` 중 입력된 값만 기록.
- 공백, 탭, `#`, 작은따옴표가 포함되거나 `=`로 시작하는 인자는 큰따옴표로 감싸며 필요한 역슬래시를 이스케이프.
- 저장 후 `ssh -G` 조회가 성공하고 지정한 값과 적용값이 다르면 알림 표시.

### 편집과 삭제

- `Include` 대상 파일을 포함해 호스트를 정의한 파일에서 직접 수정. 대상 위치의 줄만 수정, 추가, 삭제하고 나머지 바이트 보존.
- 폼에는 해당 블록의 직접 설정값을 채움. 블록에 없는 옵션의 적용값은 `inherited:`로 표시. 선택 필드의 직접 설정값을 비우면 해당 줄 삭제.
- 여러 `IdentityFile` 줄 등 폼으로 표현할 수 없는 값은 해당 필드에 `read-only`로 표시하고 원본 유지.
- 한 `Host` 줄의 여러 패턴은 옵션을 공유. 이름 변경과 삭제는 선택한 패턴에만 적용. 마지막 패턴 삭제 시 블록 전체 삭제.
- 편집 저장과 삭제 전에 변경 내용 확인. 쓰기 권한이 없는 파일의 항목은 목록에서 `read-only` 표시, 편집과 삭제 비활성.

### 쓰기 공통 규칙

- 기존 파일 교체 직전에 다시 읽어 바이트를 비교. 외부 변경을 발견하면 쓰기를 중단하고 설정을 다시 읽음.
- 기존 파일의 직전 내용은 같은 디렉터리의 `.<파일명>.sshtui.bak`에 백업하고 다음 교체 때 덮어씀. `Include conf.d/*` 같은 glob의 일반 매칭에서 빠지도록 파일명을 `.`으로 시작.
- 임시 파일에 쓴 뒤 교체. 심볼릭 링크는 유지하고 해석된 대상 파일을 교체하며, 백업도 대상 파일의 디렉터리에 생성.
- 기존 파일 권한 유지.

### 닉네임 규칙

- 새 호스트를 저장할 때 `Host`의 단일 이름으로 사용. 편집 시 닉네임 필수.
- 입력 문자 제한은 [입력 검증](#입력-검증) 참고. 새 이름은 목록에 있는 이름과 대소문자를 구분하여 비교. 기존 이름을 유지하는 편집은 중복 검사 생략.

### 알려진 제한

| 항목 | 동작 |
|---|---|
| `Include`의 `${VAR}`와 `%` 토큰 | 인자를 확장하지 않고 경고 표시 |
| `~user` | 홈 디렉터리로 확장하지 않음 |
| 블록 삭제 | 블록 위의 주석 줄 유지 |
| 하드 링크 | 파일 교체로 인해 기존 하드 링크 관계가 끊어짐 |
| `SSHTUI_SSH_DIR`와 상대 경로 `Include` | 파일이 있으면 `ssh`에 `-F <dir>/config`, 없으면 `-F none` 전달. OpenSSH의 상대 경로 `Include`는 실제 `~/.ssh` 기준이므로 목록과 적용값이 다를 수 있음 |
| 상세 열의 `Key file` | `ssh -G` 출력 사용. 설정이 없으면 SSH 기본 키 경로들이 표시될 수 있음 |

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 5. 업데이트

### 확인 방식

- 시작 시 네트워크 응답 대기 없음. 알림은 이전 실행이 남긴 캐시 기준으로만 표시.
- 마지막 시도 후 24시간 이상 경과했거나 시도 기록이 없을 때 백그라운드 확인. 요청 전에 시도 시각을 기록하므로 오프라인이거나 즉시 종료해도 24시간에 최대 1회 시도.
- `curl`로 `<저장소>/releases/latest`의 최종 리다이렉트 URL을 읽어 태그에서 버전 추출. GitHub API 미사용, 제한 시간 2초. `curl`이 없으면 자동 확인 없이 정상 실행.
- `MAJOR.MINOR.PATCH`의 세 정수를 순서대로 비교, `v` 접두사 허용. `-rc1` 등이 붙은 형식은 새 버전으로 판정하지 않음.
- 자동 확인은 하루 최대 1회 `github.com`에 요청. `SSHTUI_NO_UPDATE_CHECK=1`로 자동 확인과 알림 비활성화.

### 캐시 파일

| 플랫폼 | 경로 |
|---|---|
| Linux | `~/.cache/sshtui/update-check`. `$XDG_CACHE_HOME`이 지정된 경우 `$XDG_CACHE_HOME/sshtui/update-check` 우선 |
| macOS | `~/Library/Caches/sshtui/update-check` |
| Windows | `%LOCALAPPDATA%\sshtui\update-check` |

- 파일 형식은 `checked_at=<유닉스 초>`와 `latest=<버전>` 두 줄. 확인된 버전이 없으면 `latest=`로 기록.
- 동시 확인 제어용 `update-check.lock`은 캐시와 같은 디렉터리에 생성.
- `checked_at=0`으로 바꾸면 자동 확인이 활성화된 다음 실행에서 다시 확인.

### 적용 흐름

1. 호스트 목록에서 `u`로 `Update sshtui` 팝업 열기. 제목 오른쪽에 `<현재 버전> → <최신 버전>`, 본문에 설치 방식(`Installed via`) 표시. 설치 방식에 따라 실행할 명령(`Command`) 또는 릴리스 URL(`Download`) 표시.
2. 실행할 명령이 있으면 `Enter`로 TUI 종료와 터미널 복원 후 명령 실행. 복원된 터미널에서 `sudo` 프롬프트 입력과 설치 출력 확인 가능.
3. 성공 시 `Start sshtui again to use the new version.` 안내 후 종료.

| 화면 안내 | 실제 키 | 동작 |
|---|---|---|
| `enter update and exit` | `Enter` | TUI 종료 후 업데이트 명령 실행 |
| `esc cancel` / `esc close` | `Esc` | 목록 복귀. 실행할 명령이 없으면 `esc close` 표시 |

### 설치 방식별 명령

| 설치 방식 | 판정 근거 | 실행 명령 |
|---|---|---|
| Homebrew | 실행 파일 경로에 `/Cellar/`, `/homebrew/`, `/linuxbrew/` 중 하나 포함 | `brew upgrade sshtui` |
| cargo | `~/.cargo/bin/` 하위 | `cargo install --git <저장소> --tag v<버전> --locked` |
| install.sh 또는 수동 복사 | 그 외 홈 디렉터리 하위 | `main` 브랜치의 `install.sh`를 임시 디렉터리에 내려받은 뒤 `sh <tmp>/install.sh --bin-dir <원래 디렉터리>` |
| .deb | `/usr/bin/` 또는 `/usr/local/bin/` 하위, `/var/lib/dpkg/status` 파일 존재, 지원 아키텍처 | 릴리스 에셋을 임시 디렉터리에 내려받은 뒤 `sudo dpkg -i <tmp>/sshtui_<arch>.deb` |
| .rpm | 같은 경로, .deb 판정 조건 불충족, `/var/lib/rpm` 디렉터리 존재, 지원 아키텍처 | `sudo rpm -U <저장소>/releases/latest/download/sshtui-<arch>.rpm` |
| 불명 | 위 조건에 해당하지 않거나 실행 파일 경로 조회 실패 | 실행 없음. `Download`에 릴리스 URL 표시 |

- Windows는 `~/.cargo/bin` 하위면 cargo, 그 외는 불명으로 판정.
- 패키지 아키텍처 매핑은 `.deb`의 경우 `x86_64` → `amd64`, `aarch64` → `arm64`, `arm` → `armhf`. `.rpm`은 `x86_64`와 `aarch64` 지원.
- 각 단계의 프로그램과 인자를 셸의 명령 해석 없이 직접 전달. 직접 내려받는 파일은 실행마다 새로 만드는 임시 디렉터리에 저장.

### 전제 조건

아래 배포 구성 요소는 아직 미준비 상태. 저장소와 릴리스가 준비되기 전 확인 결과는 접속 불가. 설치 방식에 필요한 배포 요소가 준비되기 전 해당 업그레이드 명령은 실패.

| 항목 | 필요 조건 |
|---|---|
| GitHub 저장소와 릴리스 | `hungryZoo/sshtui` 저장소와 `v<버전>` 태그의 릴리스 |
| 설치 스크립트 | 저장소 `main` 브랜치의 `install.sh`, `--bin-dir` 옵션 지원 |
| Homebrew formula | `sshtui` |
| 릴리스 에셋 | 버전 없는 파일명 `sshtui_<arch>.deb`, `sshtui-<arch>.rpm` |

저장소 주소의 정의 위치는 `Cargo.toml`의 `repository` 한 곳.

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 6. 네이티브 파일 다이얼로그

### 플랫폼별 동작

| 플랫폼 | 수단 | 숨김파일 표시 | 비고 |
|---|---|---|---|
| macOS | `osascript`의 `choose file ... with invisibles` | 강제 표시 요청 | `Select an SSH private key` 프롬프트 |
| Windows | `rfd`의 `IFileOpenDialog`, `FOS_FORCESHOWHIDDEN` | 강제 표시 요청 | `set_show_hidden_files(true)` |
| Linux와 그 외 Unix | `zenity --file-selection` 또는 `kdialog --getopenfilename` | 강제 표시 옵션 미사용, 화면에 안내 표시 | KDE에서는 설치된 `kdialog` 우선. 그 외에는 `zenity` 우선, 없으면 `kdialog` |

- `zenity` 안내 문구 `Hidden files: press Ctrl+H in the dialog`.
- `kdialog` 안내 문구 `Hidden files: press Alt+. in the dialog`.
- Linux에서 `rfd`와 XDG portal을 사용하지 않는 이유는 portal의 FileChooser에 숨김파일 표시 옵션이 없고 `rfd`의 Linux 숨김파일 옵션은 GTK3 백엔드에서만 지원되기 때문.

### 공통 동작

- 시작 위치는 SSH 디렉터리. 디렉터리가 없으면 존재하는 가장 가까운 상위 디렉터리 사용.
- 취소 시 폼으로 복귀하고 기존 입력값 유지. 다이얼로그 종료 후 TUI 화면 전체 다시 그리기.

### 비활성 조건

| 조건 | 표시 문구 |
|---|---|
| SSH 세션(`SSH_CONNECTION`, `SSH_CLIENT`, `SSH_TTY` 중 하나 이상이 비어 있지 않음) | `Browse is unavailable over SSH (no local desktop session)` |
| Linux와 그 외 Unix에서 `DISPLAY`와 `WAYLAND_DISPLAY` 모두 미설정 또는 빈 값 | `Browse is unavailable: no graphical session (DISPLAY and WAYLAND_DISPLAY are not set)` |
| Linux와 그 외 Unix에서 `zenity`와 `kdialog` 모두 `PATH`에 없음 | `Browse is unavailable: install zenity or kdialog` |

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 7. 검증 상태

### 검증 결과

| 구분 | 항목 | 상태 |
|---|---|---|
| 자동 | 단위 테스트 369개(`cargo test`)로 확인한 config 읽기와 편집과 쓰기, 검증, ssh 인자, 비밀키 판별, 화면 상태, 렌더링, 마우스 클릭 영역, 업데이트 확인 | 통과 (macOS) |
| 자동 | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | 통과 |
| 자동 | `cargo check --target x86_64-pc-windows-msvc`, `--target x86_64-unknown-linux-gnu` | 통과 (타입 검사만) |
| 수동 | 임시 ssh 디렉터리에서 목록, 새 접속 저장, 편집, 삭제, 외부 변경 감지, 불일치 알림, 1회 접속의 ssh 실행과 종료 코드 | 확인 (macOS, tmux) |
| 수동 | 마우스 범위는 tmux 창에 SGR 마우스 시퀀스를 입력해 확인한 호스트 줄 클릭과 더블 클릭 접속, 휠, 키 안내, 필터 바, 필드 클릭, 팝업 바깥 클릭, 누른 곳과 다른 곳에서 뗀 경우의 취소, 종료 뒤 남는 입력 없음, `SSHTUI_NO_MOUSE` | 확인 (macOS, tmux) |
| 수동 | 디자인 목업(docs/design) 6장과의 대조: 목업과 같은 표본 호스트 12개, 100x28 창에서 목록, 필터, 새 접속, 편집 오류, 키 목록, 변경 확인 화면의 배치와 문구, 트루컬러 색 코드(배경, 강조, 삭제 줄 배경 등), COLORTERM 미설정 시 16색 전환 | 확인 (macOS, tmux) |
| 수동 | 업데이트 확인 범위는 `latest=9.9.9`일 때의 알림과 `Update sshtui` 팝업, 24시간 이내 캐시 유지, 오래된 캐시의 시도 시각 선기록, `SSHTUI_NO_UPDATE_CHECK`, `--check-update` 종료 코드 `11`(릴리스 없음, `curl` 없음), `Update sshtui` 팝업에서 `Enter` 후 터미널 복원과 명령 실행, 실패 시 종료 코드 `1` | 확인 (macOS, tmux, 임시 HOME) |
| 미검증 | 실제 릴리스 조회 성공과 새 버전 감지(저장소와 릴리스가 아직 없음) | 미검증 |
| 미검증 | 설치 방식별 업그레이드 명령의 실제 성공(Homebrew, cargo, install.sh, .deb, .rpm) | 미검증 |
| 미검증 | 네이티브 다이얼로그의 실제 표시와 숨김파일 표시(macOS, Windows, Linux) | 미검증 |
| 미검증 | 실제 서버 접속 성공(암호, 패스프레이즈, 호스트 키 확인 흐름) | 미검증 |
| 미검증 | 실제 터미널 앱에서의 색 표현과 글꼴 렌더링(목업의 JetBrains Mono, 줄 간격 24px은 터미널 설정에 따름) | 미검증 |
| 미검증 | 실제 마우스 장치와 터미널 에뮬레이터(Terminal, iTerm2, Windows Terminal 등)에서의 클릭과 휠 | 미검증 |
| 미검증 | Windows와 Linux에서의 실행(링크, 터미널 동작, 종료 코드 전달) | 미검증 |
| 미검증 | zenity의 `Ctrl+H`, kdialog의 `Alt+.` 단축키 안내 문구의 정확성 | 문서 미확인 |

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 8. 수동 확인 절차

### 임시 SSH 디렉터리 확인

아래 명령은 Bash 기준. 실제 접속 단계까지 사용자 설정 및 키 탐색을 임시 디렉터리에 한정하도록 예제 설정에 키와 호스트 키 파일 경로를 지정.

1. 임시 디렉터리와 설정, 헤더만 있는 가짜 키 생성. `web`, `dev`, `stage`와 `Host *` 블록이 든 `config`가 기대 결과.

   ```bash
   ssh_dir="$(mktemp -d)"
   cat > "$ssh_dir/config" <<EOF_CONFIG
   UserKnownHostsFile "$ssh_dir/known_hosts"
   GlobalKnownHostsFile none
   IdentityFile "$ssh_dir/id_fixture"
   IdentitiesOnly yes

   Host web
     HostName 127.0.0.1
     User testuser
     Port 2222

   Host dev stage
     HostName 127.0.0.1
     Port 2222

   Host *
     ServerAliveInterval 30
   EOF_CONFIG
   chmod 600 "$ssh_dir/config"
   printf '%s\n' '-----BEGIN OPENSSH PRIVATE KEY-----' > "$ssh_dir/id_fixture"
   chmod 600 "$ssh_dir/id_fixture"
   touch "$ssh_dir/.hidden-fixture"
   printf '%s\n' "$ssh_dir"
   ```

   상대 경로 `Include`는 OpenSSH가 실제 `~/.ssh`를 기준으로 해석하므로 예제에서 생략.

2. `SSHTUI_SSH_DIR="$ssh_dir" sshtui` 실행. 호스트 목록이 시작 화면으로 표시되는지 확인. 종료 후 재실행할 때도 같은 명령 사용.
3. `Home`으로 `web`을 선택하고 `↓`로 이동. 순서대로 `web`, `dev`, `stage`가 표시되고 `web` 상세값이 `testuser`, `127.0.0.1`, `2222`이며 `Source`가 임시 `config`의 `Host web` 줄을 가리키는지 확인.
4. `n`으로 열고 `Nickname`에 `local-test`, `Address`에 `127.0.0.1`, `Port`에 `2222`를 입력한 뒤 `Enter`. `ssh`가 종료되면 `cat "$ssh_dir/config"`와 `test -f "$ssh_dir/.config.sshtui.bak"`로 `Host local-test`가 `Host *` 앞에 들어갔고 백업이 생겼는지 확인.
5. 다시 실행해 `local-test`에서 `e`, `Port`에서 `Ctrl+U`로 비운 뒤 `2223` 입력, `Enter`. `Save local-test` 화면에서 변경 전후 줄 번호와 `-`, `+` 표시를 확인한 뒤 `y save`의 `y`. `diff -u "$ssh_dir/.config.sshtui.bak" "$ssh_dir/config"`에서 포트 줄만 다른지 확인.
6. `dev`를 선택해 `d`. `Delete dev` 화면에서 `Host dev stage`가 `Host stage`로 바뀌는지 확인하고 `y delete`의 `y`. 목록에서 `stage`가 계속 표시되는지 확인.
7. `local-test`에서 `e`로 포트를 바꾸고 `Enter`로 확인 화면을 연 상태에서, 다른 터미널에서 아래 명령을 실행. `ssh_dir`에는 1단계에서 출력한 경로를 입력. TUI에서 `y`를 누르면 `was changed by another program; reloaded, try again`이 포함된 메시지가 표시되고 편집을 적용하지 않은 상태로 다시 읽는지 확인.

   ```bash
   ssh_dir='<1단계에서 출력한 임시 디렉터리>'
   printf '\n# external change\n' >> "$ssh_dir/config"
   ```

8. 로컬 데스크톱 세션에서 `n`, `Ctrl+L`을 눌러 `id_fixture`가 표시되는지 확인. `Enter`로 선택하고 `Ctrl+O`를 눌러 시작 위치와 숨김 항목 `.hidden-fixture` 확인. Linux에서는 폼에 표시된 단축키로 숨김파일 표시 전환을 시도. 다이얼로그 취소 후 `Key file` 값이 유지되고 화면 전체가 정상적으로 표시되는지 확인.
9. SSH 세션에서 1단계와 2단계를 실행한 뒤 `n`, `Ctrl+O`. `Browse is unavailable over SSH (no local desktop session)`이 표시되고 다이얼로그가 열리지 않는지 확인.
10. 접속 가능한 테스트 서버의 주소와 필요한 `User`, `Port`를 `n` 폼에 입력하고 닉네임을 비운 채 `Enter`. `ssh`에서 인증한 뒤 원격 셸에서 `exit`. 다시 실행해 같은 입력에 `Nickname`으로 `live-test`를 지정하고 `Enter`로 저장 후 접속. 키 인증이 필요하면 `Key file`에 임시 디렉터리에 별도로 준비한 테스트 키를 지정. 두 접속이 종료된 뒤 로컬 터미널에서 입력 에코와 커서가 정상인지 확인.
11. `ssh -F "$ssh_dir/config" live-test` 실행. 같은 서버에 접속하고 원격 셸에서 `exit`로 종료할 수 있는지 확인.

### 마우스 확인

실제 터미널에서 [임시 SSH 디렉터리 확인](#임시-ssh-디렉터리-확인)의 설정과 `ssh_dir` 재사용.

1. `SSHTUI_SSH_DIR="$ssh_dir" sshtui` 실행. 호스트 줄을 클릭해 선택과 상세 열 갱신 확인. 휠을 위아래로 돌려 선택 이동 확인. `/ filter hosts` 바를 클릭하고 `web`을 입력해 목록이 좁혀지는지 확인한 뒤 `esc clear filter`로 초기화.
2. 호스트 줄의 같은 칸을 400ms 이내에 더블 클릭. TUI가 닫히고 예제 호스트를 대상으로 `ssh`가 실행되는지 확인. `ssh`가 끝난 뒤 셸에 마우스 입력 문자가 남지 않는지 확인.
3. 1단계 명령으로 다시 실행해 맨 아래 `n new`를 클릭. `New connection`에서 값을 입력한 필드를 다시 클릭해 포커스와 커서 이동 확인.
4. `Key file`에 포커스를 둔 상태에서 `ctrl+l pick key`를 클릭해 키 목록을 열고 팝업 바깥을 클릭. 키 값이 유지되고 폼으로 복귀하는지 확인. 다시 열고 `id_fixture` 줄을 클릭해 선택과 확정 확인.
5. `esc back`을 클릭해 목록 복귀 확인. 단일 클릭으로 선택을 바꾸고 다른 호스트 줄에서 왼쪽 버튼을 누른 채 이전에 선택한 줄로 옮겨 버튼을 뗀 뒤, 선택이 유지되는지 확인.
6. 캡처 중에는 일반 드래그로 터미널 텍스트가 선택되지 않는지 확인. 터미널에 맞춰 `Shift` 또는 `Option`을 누른 채 드래그해 텍스트 선택 확인. `q`로 종료 후 셸에 마우스 입력 문자가 남지 않는지 확인.
7. `SSHTUI_NO_MOUSE=1 SSHTUI_SSH_DIR="$ssh_dir" sshtui` 실행. 호스트 줄 클릭으로 TUI 선택이 바뀌지 않고 터미널의 기본 드래그로 텍스트가 선택되는지 확인. `q`로 종료.

### 업데이트 확인

위 절차에서 만든 임시 SSH 디렉터리(`ssh_dir`) 재사용. 아래 Bash 명령은 같은 셸에서 순서대로 실행하며, 각 TUI는 확인 후 `q`로 종료.

1. 임시 `HOME`과 캐시 경로 준비. 먼저 기존 Bash에서 `export ssh_dir` 실행 후 `bash`로 하위 셸 시작. 아래 명령의 `cache`는 운영체제에 맞는 경로 사용. Linux의 실제 캐시 경로를 사용하지 않도록 하위 셸에서 `XDG_CACHE_HOME` 해제.

   ```bash
   sshtui_bin="$(type -P sshtui)"
   export HOME="$(mktemp -d)"
   export SSHTUI_SSH_DIR="$ssh_dir"
   unset XDG_CACHE_HOME SSHTUI_NO_UPDATE_CHECK
   cache="$HOME/Library/Caches/sshtui/update-check"  # macOS
   # Linux: cache="$HOME/.cache/sshtui/update-check"
   mkdir -p "${cache%/*}" "$HOME/no-curl"
   ```

2. 알림 확인. 머리줄의 `update available u`와 `u`로 열린 `Update sshtui` 팝업의 최신 버전 `9.9.9`를 확인하고 `Esc`로 복귀. 종료 후 캐시의 두 값이 그대로인지 확인.

   ```bash
   printf 'checked_at=%s\nlatest=9.9.9\n' "$(date +%s)" > "$cache"
   "$sshtui_bin"
   cat "$cache"
   ```

3. 백그라운드 확인. 오래된 기록으로 실행한 뒤 바로 종료. `checked_at`이 실행 시점의 유닉스 초로 바뀌었는지 확인.

   ```bash
   printf 'checked_at=1\nlatest=9.9.9\n' > "$cache"
   "$sshtui_bin"
   cat "$cache"
   ```

4. 자동 확인 비활성화. 오래된 캐시를 다시 준비한 뒤 실행. 머리줄에 알림이 없고 종료 후 캐시의 두 값이 그대로인지 확인.

   ```bash
   printf 'checked_at=1\nlatest=9.9.9\n' > "$cache"
   SSHTUI_NO_UPDATE_CHECK=1 "$sshtui_bin"
   cat "$cache"
   ```

5. 즉시 확인. `sshtui --check-update; echo $?`와 같은 실행으로 종료 코드 확인. 릴리스가 없으면 `11`.

   ```bash
   "$sshtui_bin" --check-update; echo $?
   ```

6. 접속 불가 확인. 첫 명령은 빈 디렉터리만 `PATH`에 지정해 `curl`을 찾을 수 없게 하고, 두 번째 명령은 접속할 수 없는 로컬 프록시를 지정. `curl`의 소문자 변수 우선순위를 고려해 두 프록시 변수를 모두 설정하고 프록시 예외는 해제. 두 실행 모두 종료 코드 `11` 확인 후 `exit`로 하위 셸 종료.

   ```bash
   PATH="$HOME/no-curl" "$sshtui_bin" --check-update; echo $?
   env -u no_proxy -u NO_PROXY https_proxy=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9 \
     "$sshtui_bin" --check-update; echo $?
   exit
   ```

임시 `HOME`을 사용하면 홈 경로가 필요한 설치 방식 판정도 해당 `HOME` 기준.

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 9. 개발

### 모듈 구성

| 경로 | 역할 |
|---|---|
| `src/main.rs`, `src/lib.rs` | CLI 진입점, 라이브러리 모듈 선언 |
| `src/app/` | 화면 상태, 키 입력, 폼과 변경 확인 |
| `src/ui/` | 호스트 목록(`list.rs`), 접속 폼(`form.rs`), 변경 확인과 팝업(`overlays.rs`), 레이아웃(`layout.rs`), 테마(`theme.rs`), 공통 렌더링과 클릭 영역(`mod.rs`), 렌더링 테스트(`tests.rs`) |
| `src/tui.rs` | 터미널 초기화와 복원, 이벤트 루프 |
| `src/config/` | 설정 파싱, `Include` 확장, 편집, diff, 백업과 쓰기 |
| `src/ssh/` | 인자 구성, 적용값 조회, `ssh` 실행 |
| `src/dialog/` | 플랫폼별 파일 다이얼로그 |
| `src/keys.rs` | 키 파일 탐색과 비밀키 헤더 판별 |
| `src/paths.rs` | SSH 디렉터리와 설정 경로 결정 |
| `src/validate.rs` | 폼 입력 검증 |
| `src/update.rs` | 새 버전 확인, 캐시, 설치 방식 판정, 업그레이드 명령 |
| `src/system.rs` | 파일 시스템, `ssh`, 파일 다이얼로그 연동 |

디자인 원본은 `docs/design/`에 있는 Claude Design 캔버스에서 가져온 `.dc.html` 목업 6장과 `canvas.json`.

### 검증 명령

```bash
cargo fmt --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline
```

### 작업 방식

- 공통 규칙은 `AGENTS.md`, 작업별 지시문은 `tasks/`, 실행기는 `tasks/run.sh`.
- 테스트는 실제 `~/.ssh`를 사용하지 않고 `tempfile`의 임시 디렉터리를 사용. 코드에서는 `SshPaths`로 경로를 주입하고 CLI를 실행하는 확인에서는 `SSHTUI_SSH_DIR`로 임시 경로를 지정.

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>
