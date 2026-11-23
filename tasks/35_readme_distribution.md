# 작업 35: README를 배포 기준으로 재구성

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 코드는 수정하지 않는다.

## 반드시 먼저 읽을 참조 자료

- `logs/reference/skills/readme-wiki-style/SKILL.md`와 `reference/README.tu.md`: 구조의 기준. 머리 부분(가운데 정렬), 위키형 본문, 앵커 규칙, 검증 방법을 모두 따른다.
- `logs/reference/skills/cross-platform-cli-distribution/SKILL.md`의 2절(버전 없는 설치 명령)과 3절(지원 플랫폼 표기), macOS 참고
- `logs/reference/skills/homebrew-install-docs/SKILL.md`: Homebrew 설치 문단의 작성법(전체 이름 사용)
- 작업 34의 결과물: `install.sh`, `install.ps1`, `scripts/`, `.github/workflows/`, `Cargo.toml`, `.github/release-notes/v0.1.0.md`

## 목표

`README.md`를 readme-wiki-style 구조로 재구성하고, 여러 운영체제와 여러 셸에서의 설치 방법과 지원 플랫폼을 문서화한다. 본문 언어는 지금처럼 한국어(명사형 개조식, 비유 금지, 원 숫자 기호 금지, 범위는 `-`). 기존 내용(사용법, ssh config 연동, 호스트 키 삭제, 키 로그인 설정, 히스토리, 표시 언어, 업데이트, 다이얼로그, 검증 상태, 수동 확인 절차)은 버리지 않고 새 구조의 알맞은 절로 옮긴다.

## 대상 파일

- `README.md`

## 구조

### 머리 부분 (`<div align="center">`)

스킬의 순서대로.

1. 제목 `` # `sshtui` ``
2. 굵은 한 줄(무엇인지)과 일반 한 줄(무엇을 하는지)
3. 배지 줄: 릴리스, 라이선스(MIT), 사용 기술(`Rust · ratatui`). 같은 shields.io 스타일(`flat-square`)
4. 설치 명령 하나(권장 경로): `curl -fsSL https://raw.githubusercontent.com/hungryZoo/sshtui/main/install.sh | sh`
5. 플랫폼 배지: macOS, Linux, Windows, Debian과 Ubuntu(.deb), Fedora와 RHEL 계열(.rpm), Raspberry Pi. 스킬 3절의 형식(한 줄에 2개, 4개 이상이면 `<br/>`), 각 배지는 `6. 지원 플랫폼`의 해당 절로 링크
6. 스크린샷 자리: 이미지 파일이 아직 없으므로 `<img>`를 넣지 않는다. 대신 div를 닫은 뒤 실제 목록 화면을 옮긴 `text` 코드 블록 하나를 둔다(아래 "화면 예시" 사용)
7. `---` 뒤에 실행하면 무슨 일이 일어나는지 4-6줄의 소개

데모 영상은 없으므로 넣지 않는다.

화면 예시(영어 표시, 100칸 폭에서 실제로 캡처한 배치를 그대로 쓴다. 호스트 이름과 주소는 예시 값):

```text
  sshtui  3 hosts                                                                  2 suggestions i

   / filter hosts
  ────────────────────────────────────────────────────────────────────────────────────────────────
  › web             deploy@example.com                │  web
    db              10.0.0.5                          │  deploy@example.com
    bastion         ops@bastion.example.com           │
    + New connection                                  │  User       deploy
                                                      │  Address    example.com
                                                      │  Port       22
                                                      │  Key file   ~/.ssh/id_ed25519
                                                      │  Source     ~/.ssh/config:1
                                                      │
                                                      │  enter runs
                                                      │  $ ssh web
  ────────────────────────────────────────────────────────────────────────────────────────────────
   enter  connect   e edit   d delete   x forget(known_host)   / filter   r reload   esc quit
```

### 본문

`## Table of Contents`(번호 목록과 하위 절 링크) 뒤에 번호가 붙은 최상위 절. 각 최상위 절의 끝에 `<p align="right"><a href="#table-of-contents">↑ back to top</a></p>`.

1. `## 1. 개요`: 기능, 동작 방식 표, 범위 외 기능
2. `## 2. 설치`: 설치 방법만 다룬다. 순서는 다음과 같다.
   - 설치 스크립트 (권장): macOS와 Linux. `curl ... | sh` 한 줄. 같은 명령이 bash, zsh, fish, ksh, dash 등 어느 셸에서나 동작한다는 점(스크립트는 `sh`가 실행). `curl`이 없을 때의 `wget -qO- ... | sh`. 옵션과 환경변수 표(`install.sh --help`와 대조). 설치 위치와 PATH 안내를 셸별 표로(bash와 zsh, fish, csh와 tcsh, nushell). 설정 파일을 자동으로 고치지 않는다는 점
   - Windows (PowerShell): `irm https://raw.githubusercontent.com/hungryZoo/sshtui/main/install.ps1 | iex`. Windows PowerShell 5.1과 PowerShell 7 공통. `cmd.exe`에서 실행하는 한 줄(`powershell -NoProfile -ExecutionPolicy Bypass -Command "irm ... | iex"`). 환경변수 표, 설치 위치, PATH 처리
   - Homebrew: 스킬의 포뮬러 문단 형식. 전체 이름 `hungryZoo/tap/sshtui`
   - Linux 패키지 (.deb / .rpm): 버전 없는 `releases/latest/download/` URL로 아키텍처별 명령 표
   - 수동 설치: 릴리스 페이지의 자산 이름 형식(`sshtui-<version>-<triple>.tar.gz`), 압축 해제와 배치
   - 소스에서 설치: `cargo install --git https://github.com/hungryZoo/sshtui --locked`와 저장소 클론 후 `cargo install --path .`
   - 검증하며 설치: 스크립트를 내려받아 읽고 실행, `SHA256SUMS` 확인(`sha256sum -c SHA256SUMS --ignore-missing`, macOS는 `shasum -a 256 -c`), 특정 버전 고정(`SSHTUI_VERSION`), `gh attestation verify`
   - 업데이트와 삭제: 설치 방식별 업데이트 방법(앱 안의 `u`, `sshtui --update`, Windows는 설치 명령 재실행)과 삭제 방법 표
3. `## 3. 빠른 시작`: 실행 명령 하나와 그 뒤의 흐름(목록, new 줄, 폼, 저장 후 접속)을 짧게
4. `## 4. 사용법`: 기존 `사용법`의 하위 절(공통 화면 구성, 호스트 목록, 접속 폼, 키 로그인 설정, 셸 히스토리 가져오기, 호스트 키 삭제, 변경 확인, 키 목록, 불일치 알림, 마우스, 접속 실행, 입력 검증)과 기존 `ssh config 연동`, `업데이트`, `네이티브 파일 다이얼로그`의 내용을 하위 절로 정리. 키와 마우스 참조 표 유지. 히스토리 파일 표는 작업 33의 11개 항목으로 갱신(`src/history.rs`와 대조)
5. `## 5. 설정`: 환경변수 표(`--help`와 대조), 실행 옵션 표, 표시 언어, 색, 상태 파일과 캐시 파일 위치
6. `## 6. 지원 플랫폼`: 대상 표(triple, 용도, 정적 여부), 정적 링크와 macOS 최소 버전(11.0)에 대한 한 줄, Raspberry Pi 모델별 표(모델, CPU, 64비트 OS와 32비트 OS에서의 자산), Windows 참고(OpenSSH 클라이언트 필요, VC++ 재배포 패키지 불필요), macOS 참고(브라우저로 받은 파일의 격리 속성 해제 한 줄), 지원하는 셸 표(설치 명령과 히스토리 후보 각각의 지원 여부)
   - 중요: 이 절의 모든 플랫폼에 "빌드만 됨", "macOS에서 확인" 같은 확인 수준을 적는다. 릴리스가 아직 없으므로 실제 릴리스 자산을 내려받아 확인한 것처럼 쓰지 않는다
7. `## 7. 검증 상태`: 기존 `검증 상태`와 `수동 확인 절차`. 이 파일 끝의 "검수자 확인 내용"을 반영
8. `## 8. 개발`: 저장소 구성, 검증 명령, 릴리스 만드는 방법(`scripts/`, 태그 푸시, 릴리스 노트 파일, Homebrew 탭 갱신), 작업 방식
9. `## 9. 라이선스`: MIT

## 규칙

- 명령에 버전 숫자를 직접 쓰지 않는다. `latest` URL이나 `<version>` 표기를 쓴다.
- Homebrew 명령은 모두 전체 이름으로 쓴다.
- 모든 주장(키, 옵션, 환경변수, 설치 스크립트 옵션, 자산 이름, 워크플로 동작)을 소스와 대조한다.
- 아직 존재하지 않는 것을 존재하는 것처럼 쓰지 않는다. 저장소 `hungryZoo/sshtui`와 첫 릴리스는 아직 게시되지 않았다. `2. 설치` 맨 위에 "첫 릴리스 게시 전에는 설치 스크립트, Homebrew, 패키지 경로가 동작하지 않으며 소스에서 설치만 가능"이라는 한 줄을 넣는다(게시 뒤 지울 문장임을 주석 `<!-- -->`으로 표시).
- 배지 URL의 이스케이프 규칙(`_`는 `__`, `-`는 `--`, `·`는 `%C2%B7`)을 지킨다. 가운뎃점은 배지 URL과 화면 문구 인용 안에서만 허용.
- 앵커 규칙에 따라 목차와 본문 내부 링크, 배지 링크를 전부 맞춘다. 한글 제목의 앵커는 GitHub 규칙(소문자화, 공백은 `-`, 구두점 제거)으로 계산한다.

## 검수자 확인 내용

검수자가 macOS(Apple Silicon) 개발 장비에서 직접 확인한 내용이다. `7. 검증 상태`의 수동 행과 `6. 지원 플랫폼`의 확인 수준에 그대로 반영한다. Codex가 스스로 수행한 확인은 수동 행으로 적지 않는다.

확인한 것(상태 표기 `확인 (macOS)`):

- 빌드: `scripts/build-all.sh`로 6개 대상 릴리스 빌드 성공(zig 0.16.0, cargo-zigbuild 0.22.3). `file` 출력 기준 Linux 4개 대상은 정적 링크. ARM 속성 섹션을 해석해 `armv7-unknown-linux-musleabihf` 자산은 `Tag_CPU_arch` v7, `arm-unknown-linux-musleabihf` 자산은 v6임을 확인. macOS 2개 대상의 `minos`는 11.0, arm64 바이너리의 `codesign -v` 통과
- 실행: `aarch64-apple-darwin` 바이너리의 `--version`을 직접 실행, `x86_64-apple-darwin` 바이너리는 Rosetta에서 `--version` 실행
- 패키징: `scripts/package-all.sh`로 `dist/`에 자산 19개 생성(tar.gz 6개, deb 3개와 별칭 3개, rpm 2개와 별칭 2개, `install.sh`, `install.ps1`, `SHA256SUMS`). `SHA256SUMS`의 18개 항목 검증 통과. `.deb`의 `control` 필드(`Package`, `Architecture: armhf`, `Depends: openssh-client`)와 파일 배치, `.rpm`의 파일 목록 확인
- 설치 스크립트: `SSHTUI_DOWNLOAD_BASE=file://.../dist`와 임시 `HOME`으로 `dash install.sh`와 `sh install.sh` 실행. 지정 디렉터리와 기본 `~/.local/bin` 설치, 설치 후 `--version` 일치, 셸별 PATH 안내(zsh의 `export`, fish의 `fish_add_path`, tcsh의 `set path`), 체크섬 불일치 시 종료 코드 1과 설치 없음, 기준 URL만 주고 버전을 주지 않았을 때의 오류
- Homebrew 포뮬러: `scripts/update-formula.sh`로 생성한 파일의 Ruby 구문 검사 통과

미검증(상태 표기 `미검증`):

- Windows: 릴리스 빌드, zip, `install.ps1` 실행(로컬에 PowerShell 없음). `cargo check --target`의 타입 검사만 통과
- Linux: 바이너리와 패키지를 만들기만 했고 실제 Linux에서 실행, `.deb`와 `.rpm` 설치는 하지 않음. Raspberry Pi 실기기 미확인
- GitHub Actions 워크플로(`ci.yml`, `release.yml`)는 한 번도 실행되지 않음. `shellcheck`는 로컬에 없어 미실행
- 실제 릴리스에서의 `curl ... | sh`, `irm ... | iex`, Homebrew 설치, `releases/latest` 경로, 증명 검증

`6. 지원 플랫폼`의 확인 수준 열은 다음 값만 쓴다: `실행 확인 (macOS)`(aarch64-apple-darwin), `Rosetta에서 실행 확인`(x86_64-apple-darwin), `빌드만 확인`(Linux 4개), `미검증, CI에서 빌드 예정`(Windows 2개).

## 완료 조건

- 목차, 본문 내부 링크, 배지 링크의 앵커가 모두 실제 제목과 일치(스크립트로 확인하고 결과를 최종 보고에 적는다)
- 원 숫자 기호 없음. 가운뎃점은 허용 범위에만 있음. `PTY` 문자열 없음
- 버전 숫자가 직접 들어간 설치 명령 없음
- `git status`에 `README.md`만 변경으로 표시
