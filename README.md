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
   - [호스트 목록](#호스트-목록)
   - [접속 폼](#접속-폼)
   - [변경 확인](#변경-확인)
   - [키 목록](#키-목록)
   - [불일치 알림](#불일치-알림)
   - [접속 실행](#접속-실행)
   - [입력 검증](#입력-검증)
   - [환경변수](#환경변수)
4. [ssh config 연동](#4-ssh-config-연동)
   - [읽기](#읽기)
   - [새 호스트 저장](#새-호스트-저장)
   - [편집과 삭제](#편집과-삭제)
   - [쓰기 공통 규칙](#쓰기-공통-규칙)
   - [닉네임 규칙](#닉네임-규칙)
   - [알려진 제한](#알려진-제한)
5. [네이티브 파일 다이얼로그](#5-네이티브-파일-다이얼로그)
   - [플랫폼별 동작](#플랫폼별-동작)
   - [공통 동작](#공통-동작)
   - [비활성 조건](#비활성-조건)
6. [검증 상태](#6-검증-상태)
   - [검증 결과](#검증-결과)
7. [수동 확인 절차](#7-수동-확인-절차)
   - [임시 SSH 디렉터리 확인](#임시-ssh-디렉터리-확인)
8. [개발](#8-개발)
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

모든 화면에서 `Ctrl+C`로 종료.

### 호스트 목록

시작 화면. 목록에 표시할 호스트가 없으면 `New connection`으로 시작.

상세 패널에 `Alias`, `User`, `Address`, `Port`, `Key file`, `Source`(파일과 줄 번호) 표시. 적용값 조회 실패 시 `Effective settings:` 뒤에 오류 표시. 변경할 수 없는 파일의 호스트는 목록에 `[ro]`, 상세 패널에 `Read-only file` 표시.

| 키 | 동작 |
|---|---|
| `↑`, `k` / `↓`, `j` | 이전 / 다음 호스트 선택 |
| `Home`, `g` / `End`, `G` | 첫 / 마지막 호스트 선택 |
| `Enter` | 선택한 호스트로 접속 |
| `n` / `e` / `d` | 새 접속 폼 / 선택 항목 편집 / 삭제 확인 |
| `/` | 필터 입력 시작 |
| `↑`, `↓` (필터 입력 중) | 필터 결과에서 선택 이동 |
| `Enter` (필터 입력 중) | 필터 유지, 목록 키로 복귀 |
| `Esc` | 필터 초기화 |
| `r` / `q` | 설정 다시 읽기 / 종료 |

### 접속 폼

| 필드 | 새 접속 | 편집 |
|---|---|---|
| `Nickname` | 선택 | 필수 |
| `User` | 선택 | 선택 |
| `Address` | 필수 | 편집 가능한 기존 `HostName` 값은 필수. 없으면 생략 가능 |
| `Port` | 선택 | 선택 |
| `Key file` | 선택 | 선택 |

| 키 | 동작 |
|---|---|
| `Tab`, `↓` / `Shift+Tab`, `↑` | 다음 / 이전 필드 |
| `←`, `→`, `Home`, `End` | 필드 내 커서 이동 |
| `Backspace`, `Delete` / `Ctrl+U` | 문자 삭제 / 필드 비우기 |
| `Enter` (새 접속, 닉네임 없음) | 저장 없이 즉시 접속 |
| `Enter` (새 접속, 닉네임 있음) | 저장 후 닉네임으로 접속. 적용값 불일치 시 먼저 알림 표시 |
| `Enter` (편집) | 변경 확인 화면 열기. 변경이 없으면 `No changes`를 표시하고 목록으로 복귀 |
| `Esc` | 목록 복귀. 표시할 호스트가 없으면 종료 |
| `Ctrl+L`, `F2` | SSH 디렉터리에서 찾은 비밀키 목록 열기 |
| `Ctrl+O`, `F3` | 네이티브 파일 다이얼로그 열기 |

- `Command:` 줄에 실시간 명령 미리보기 또는 검증 오류 표시. 닉네임으로 접속하는 경우 닉네임이 대상.
- 1회 접속에서 빈 `User`는 주소의 `user@` 부분 생략, 빈 `Port`와 `Key file`은 각각 `-p`와 `-i` 생략. 새 호스트 저장 시 빈 필드에 해당하는 설정 옵션도 생략.
- 키 파일 지정 방법은 키 목록 선택, 파일 다이얼로그 선택, `Key file` 직접 입력. 직접 입력에 사용한 `~` 또는 `~/` 경로는 1회 접속 인자를 만들 때 홈 디렉터리로 확장. 저장 시에는 입력한 경로 표기를 설정 파일에 기록.

### 변경 확인

편집은 `Save <닉네임>`, 삭제는 `Delete <닉네임>` 화면에 대상 파일과 변경 전후 줄 표시.

| 키 | 동작 |
|---|---|
| `↑`, `↓` | 줄 단위 스크롤 |
| `PageUp`, `PageDown` | 페이지 단위 스크롤 |
| `y`, `Enter` | 변경 사항 기록 |
| `n`, `Esc` | 취소. 편집은 폼, 삭제는 목록으로 복귀 |

### 키 목록

`Select a key file` 화면. SSH 디렉터리에서 비밀키 헤더로 판별한 파일을 표시.

| 키 | 동작 |
|---|---|
| `↑`, `k` / `↓`, `j` | 이전 / 다음 키 선택 |
| `Enter` | `Key file`에 선택 경로 입력 후 폼으로 복귀 |
| `Esc` | 기존 값 유지 후 폼으로 복귀 |

### 불일치 알림

`Saved settings differ from effective SSH settings` 화면. 저장 후 `ssh -G` 조회가 성공하고 폼에서 지정한 값과 적용값이 다를 때 차이 표시.

| 키 | 동작 |
|---|---|
| `Enter` | 새 호스트 저장 뒤에는 닉네임으로 접속, 편집 뒤에는 목록으로 복귀 |
| `Esc` | 목록으로 복귀, 이미 저장한 변경 유지 |

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

### 환경변수

| 이름 | 동작 |
|---|---|
| `SSHTUI_SSH_DIR` | SSH 디렉터리 변경. 미설정 또는 빈 값이면 `~/.ssh` 사용. 상대 경로는 현재 작업 디렉터리 기준 |

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 4. ssh config 연동

### 읽기

- SSH 디렉터리의 `config`에서 `Include`를 재귀적으로 읽고 glob을 확장. 상대 경로는 SSH 디렉터리 기준, `~`와 `~/` 경로는 홈 디렉터리 기준.
- `Host`의 개별 패턴을 목록에 표시. `*` 또는 `?`가 포함된 패턴, `!`로 시작하는 패턴, `Match` 헤더는 목록에서 제외.
- 항목별 출처 파일과 `Host` 줄 번호 표시. 상세 패널의 적용값은 `ssh -G <닉네임>` 결과 사용.

### 새 호스트 저장

- 루트 `config`가 없으면 생성. Unix에서 새 파일 권한은 `600`, 새 디렉터리 생성 모드는 `700`(umask 적용).
- 와일드카드 또는 부정 패턴이 있는 첫 `Host` 블록이나 첫 `Match` 블록 중 먼저 나오는 블록 앞에 삽입. 블록에 붙은 주석이 있으면 주석 앞에 삽입. 해당 블록이 없으면 파일 끝에 추가. SSH가 옵션별로 먼저 얻은 값을 적용하는 순서를 고려한 배치.
- `HostName`, `User`, `Port`, `IdentityFile` 중 입력된 값만 기록.
- 공백, 탭, `#`, 작은따옴표가 포함되거나 `=`로 시작하는 인자는 큰따옴표로 감싸며 필요한 역슬래시를 이스케이프.
- 저장 후 `ssh -G` 조회가 성공하고 지정한 값과 적용값이 다르면 알림 표시.

### 편집과 삭제

- `Include` 대상 파일을 포함해 호스트를 정의한 파일에서 직접 수정. 대상 위치의 줄만 수정, 추가, 삭제하고 나머지 바이트 보존.
- 폼에는 해당 블록의 직접 설정값을 채움. 블록에 없는 옵션의 적용값은 `inherited:`로 표시. 선택 필드의 직접 설정값을 비우면 해당 줄 삭제.
- 여러 `IdentityFile` 줄 등 폼으로 표현할 수 없는 값은 해당 필드에 `[read-only]`로 표시하고 원본 유지.
- 한 `Host` 줄의 여러 패턴은 옵션을 공유. 이름 변경과 삭제는 선택한 패턴에만 적용. 마지막 패턴 삭제 시 블록 전체 삭제.
- 편집 저장과 삭제 전에 변경 내용 확인. 쓰기 권한이 없는 파일의 항목은 목록에서 `[ro]` 표시, 편집과 삭제 비활성.

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
| 상세 패널의 `Key file` | `ssh -G` 출력 사용. 설정이 없으면 SSH 기본 키 경로들이 표시될 수 있음 |

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 5. 네이티브 파일 다이얼로그

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

## 6. 검증 상태

### 검증 결과

| 구분 | 항목 | 상태 |
|---|---|---|
| 자동 | 단위 테스트 280개(`cargo test`): config 읽기와 편집과 쓰기, 검증, ssh 인자, 비밀키 판별, 화면 상태, 렌더링 | 통과 (macOS) |
| 자동 | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | 통과 |
| 자동 | `cargo check --target x86_64-pc-windows-msvc`, `--target x86_64-unknown-linux-gnu` | 통과 (타입 검사만) |
| 수동 | 임시 ssh 디렉터리에서 목록, 새 접속 저장, 편집, 삭제, 외부 변경 감지, 불일치 알림, 1회 접속의 ssh 실행과 종료 코드 | 확인 (macOS, tmux) |
| 미검증 | 네이티브 다이얼로그의 실제 표시와 숨김파일 표시(macOS, Windows, Linux) | 미검증 |
| 미검증 | 실제 서버 접속 성공(암호, 패스프레이즈, 호스트 키 확인 흐름) | 미검증 |
| 미검증 | Windows와 Linux에서의 실행(링크, 터미널 동작, 종료 코드 전달) | 미검증 |
| 미검증 | zenity의 `Ctrl+H`, kdialog의 `Alt+.` 단축키 안내 문구의 정확성 | 문서 미확인 |

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 7. 수동 확인 절차

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
5. 다시 실행해 `local-test`에서 `e`, `Port`에서 `Ctrl+U`로 비운 뒤 `2223` 입력, `Enter`. `Save local-test` 화면에서 변경 줄을 확인한 뒤 `y`. `diff -u "$ssh_dir/.config.sshtui.bak" "$ssh_dir/config"`에서 포트 줄만 다른지 확인.
6. `dev`를 선택해 `d`. `Delete dev` 화면에서 `Host dev stage`가 `Host stage`로 바뀌는지 확인하고 `y`. 목록에서 `stage`가 계속 표시되는지 확인.
7. `local-test`에서 `e`로 포트를 바꾸고 `Enter`로 확인 화면을 연 상태에서, 다른 터미널에서 아래 명령을 실행. `ssh_dir`에는 1단계에서 출력한 경로를 입력. TUI에서 `y`를 누르면 `was changed by another program; reloaded, try again`이 포함된 메시지가 표시되고 편집을 적용하지 않은 상태로 다시 읽는지 확인.

   ```bash
   ssh_dir='<1단계에서 출력한 임시 디렉터리>'
   printf '\n# external change\n' >> "$ssh_dir/config"
   ```

8. 로컬 데스크톱 세션에서 `n`, `Ctrl+L`을 눌러 `id_fixture`가 표시되는지 확인. `Enter`로 선택하고 `Ctrl+O`를 눌러 시작 위치와 숨김 항목 `.hidden-fixture` 확인. Linux에서는 폼에 표시된 단축키로 숨김파일 표시 전환을 시도. 다이얼로그 취소 후 `Key file` 값이 유지되고 화면 전체가 정상적으로 표시되는지 확인.
9. SSH 세션에서 1단계와 2단계를 실행한 뒤 `n`, `Ctrl+O`. `Browse is unavailable over SSH (no local desktop session)`이 표시되고 다이얼로그가 열리지 않는지 확인.
10. 접속 가능한 테스트 서버의 주소와 필요한 `User`, `Port`를 `n` 폼에 입력하고 닉네임을 비운 채 `Enter`. `ssh`에서 인증한 뒤 원격 셸에서 `exit`. 다시 실행해 같은 입력에 `Nickname`으로 `live-test`를 지정하고 `Enter`로 저장 후 접속. 키 인증이 필요하면 `Key file`에 임시 디렉터리에 별도로 준비한 테스트 키를 지정. 두 접속이 종료된 뒤 로컬 터미널에서 입력 에코와 커서가 정상인지 확인.
11. `ssh -F "$ssh_dir/config" live-test` 실행. 같은 서버에 접속하고 원격 셸에서 `exit`로 종료할 수 있는지 확인.

<p align="right"><a href="#table-of-contents">↑ back to top</a></p>

## 8. 개발

### 모듈 구성

| 경로 | 역할 |
|---|---|
| `src/main.rs`, `src/lib.rs` | CLI 진입점, 라이브러리 모듈 선언 |
| `src/app/` | 화면 상태, 키 입력, 폼과 변경 확인 |
| `src/ui/` | 목록, 폼, 오버레이 렌더링 |
| `src/tui.rs` | 터미널 초기화와 복원, 이벤트 루프 |
| `src/config/` | 설정 파싱, `Include` 확장, 편집, diff, 백업과 쓰기 |
| `src/ssh/` | 인자 구성, 적용값 조회, `ssh` 실행 |
| `src/dialog/` | 플랫폼별 파일 다이얼로그 |
| `src/keys.rs` | 키 파일 탐색과 비밀키 헤더 판별 |
| `src/paths.rs` | SSH 디렉터리와 설정 경로 결정 |
| `src/validate.rs` | 폼 입력 검증 |
| `src/system.rs` | 파일 시스템, `ssh`, 파일 다이얼로그 연동 |

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
