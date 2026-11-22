# 작업 13: README에 업데이트 기능 반영

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `tasks/11_readme.md`의 "작성 원칙"과 "구조 (위키형)"을 그대로 적용한다(한국어, 명사형 개조식, 비유 금지, 가운뎃점과 원 숫자 기호 금지, 범위는 `-`, 번호 붙은 최상위 절, 절 끝의 back to top 링크, 목차 앵커 일치).

## 목표

작업 12에서 추가한 새 버전 확인과 업데이트 기능을 `README.md`에 반영한다. 코드는 수정하지 않는다. 모든 내용은 `src/update.rs`, `src/main.rs`, `src/app/mod.rs`, `src/ui/`와 대조해 확인한다.

## 대상 파일

- `README.md`

## 변경 내용

1. 머리말 아래 소개 문단은 그대로 둔다.
2. `3. 사용법`
   - 옵션 표에 `--check-update`(종료 코드 0 최신, 10 업데이트 있음, 11 접속 불가), `--update`(0 완료 또는 최신, 1 실패 또는 설치 방식 불명, 11 접속 불가) 추가
   - 호스트 목록 키 표에 `u`(새 버전이 있을 때만 Update 화면 열기) 추가. 제목 줄 오른쪽의 알림 문구(`v<버전> available (u: update)`) 설명 한 줄
   - 환경변수 표에 `SSHTUI_NO_UPDATE_CHECK`(비어 있지 않고 `0`이 아닌 값이면 자동 확인과 알림을 끔. `--check-update`와 `--update`는 영향 없음) 추가
3. 새 최상위 절 `## 5. 업데이트`를 `4. ssh config 연동` 뒤에 추가하고 뒤따르는 절 번호를 하나씩 밀어 올린다(네이티브 파일 다이얼로그는 6, 검증 상태는 7, 수동 확인 절차는 8, 개발은 9). 목차와 본문 안의 앵커를 모두 갱신한다. 하위 절:
   - 확인 방식
     - 시작 시 네트워크 응답을 기다리지 않음. 알림은 이전 실행이 남긴 캐시로만 표시
     - 캐시가 24시간 이상 지났을 때만 백그라운드에서 확인. 요청 전에 시도 시각을 먼저 기록하므로 오프라인이나 즉시 종료에도 하루 1회만 시도
     - 조회 수단: `curl`로 `<저장소>/releases/latest`의 리다이렉트 URL을 읽어 태그에서 버전 추출(GitHub API 미사용, 제한 시간 2초). `curl`이 없으면 확인 없이 계속 동작
     - 버전 비교: `MAJOR.MINOR.PATCH` 세 정수 비교, `v` 접두사 허용, 그 외 형식(`-rc1` 등)은 새 버전으로 보지 않음
     - 네트워크 접근 고지: 자동 확인은 하루 최대 1회 `github.com`에 요청을 보냄. 끄는 방법은 `SSHTUI_NO_UPDATE_CHECK`
   - 캐시 파일: 플랫폼별 경로 표(Linux `~/.cache/sshtui/update-check`와 `$XDG_CACHE_HOME` 우선, macOS `~/Library/Caches/sshtui/update-check`, Windows `%LOCALAPPDATA%\sshtui\update-check`), 형식(`checked_at=<유닉스 초>`, `latest=<버전>` 두 줄), 같은 디렉터리의 `update-check.lock`, `checked_at=0`으로 바꾸면 다음 실행에서 다시 확인한다는 점
   - 적용 흐름: 목록에서 `u`, Update 화면에 현재 버전과 최신 버전과 설치 방식과 실행할 명령 표시, `Enter`로 TUI 종료와 터미널 복원 후 명령 실행(TUI 안에서 실행하지 않는 이유: `sudo` 프롬프트와 설치 출력), 성공 시 다시 실행하라는 안내 후 종료, `Esc`로 취소
   - 설치 방식별 명령 표: 판정 근거와 실행 명령(소스의 `classify_install`과 `upgrade_steps` 기준)
     - Homebrew: 경로에 `/Cellar/`, `/homebrew/`, `/linuxbrew/` → `brew upgrade sshtui`
     - cargo: `~/.cargo/bin/` → `cargo install --git <저장소> --tag v<버전> --locked`
     - install.sh 또는 수동 복사: 그 외 홈 디렉터리 하위 → `install.sh`를 임시 디렉터리에 내려받아 `sh <tmp>/install.sh --bin-dir <원래 디렉터리>` 실행
     - .deb: `/usr/bin/` 또는 `/usr/local/bin/`과 `/var/lib/dpkg/status` 존재 → 패키지를 임시 디렉터리에 내려받아 `sudo dpkg -i`
     - .rpm: 같은 경로와 `/var/lib/rpm` 존재 → `sudo rpm -U <저장소>/releases/latest/download/sshtui-<arch>.rpm`
     - 불명: 실행 없음, 릴리스 URL만 표시
     - Windows: `~/.cargo/bin` 하위면 cargo, 그 외 불명
     - 명령은 셸을 거치지 않고 단계별로 실행하며, 내려받는 파일은 실행마다 새로 만드는 임시 디렉터리에 둔다는 점
   - 전제 조건(굵게 강조하지 말고 표로): 아래는 아직 존재하지 않으며 준비되기 전에는 확인 결과가 "접속 불가"이고 업그레이드 명령은 실패함
     - GitHub 저장소 `hungryZoo/sshtui`와 `v<버전>` 태그의 릴리스
     - 저장소 `main` 브랜치의 `install.sh`(`--bin-dir` 옵션 지원)
     - Homebrew formula `sshtui`
     - 릴리스 에셋 `sshtui_<arch>.deb`, `sshtui-<arch>.rpm`(버전 없는 이름)
     - 저장소 주소는 `Cargo.toml`의 `repository` 한 곳에서 정의
4. `검증 상태` 표를 아래 내용으로 교체한다(문장 다듬기는 허용, 내용 변경 금지).

| 구분 | 항목 | 상태 |
|---|---|---|
| 자동 | 단위 테스트 314개(`cargo test`): config 읽기와 편집과 쓰기, 검증, ssh 인자, 비밀키 판별, 화면 상태, 렌더링, 업데이트 확인 | 통과 (macOS) |
| 자동 | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | 통과 |
| 자동 | `cargo check --target x86_64-pc-windows-msvc`, `--target x86_64-unknown-linux-gnu` | 통과 (타입 검사만) |
| 수동 | 임시 ssh 디렉터리에서 목록, 새 접속 저장, 편집, 삭제, 외부 변경 감지, 불일치 알림, 1회 접속의 ssh 실행과 종료 코드 | 확인 (macOS, tmux) |
| 수동 | 업데이트: 캐시에 `latest=9.9.9`를 넣었을 때의 알림과 Update 화면, 24시간 이내 캐시의 무변화, 오래된 캐시의 시도 시각 선기록, `SSHTUI_NO_UPDATE_CHECK`, `--check-update`의 종료 코드 11(릴리스 없음, `curl` 없음), Update 화면에서 Enter 후 터미널 복원과 명령 실행과 실패 시 종료 코드 1 | 확인 (macOS, tmux, 임시 HOME) |
| 미검증 | 실제 릴리스 조회 성공과 새 버전 감지(저장소와 릴리스가 아직 없음) | 미검증 |
| 미검증 | 설치 방식별 업그레이드 명령의 실제 성공(Homebrew, cargo, install.sh, .deb, .rpm) | 미검증 |
| 미검증 | 네이티브 다이얼로그의 실제 표시와 숨김파일 표시(macOS, Windows, Linux) | 미검증 |
| 미검증 | 실제 서버 접속 성공(암호, 패스프레이즈, 호스트 키 확인 흐름) | 미검증 |
| 미검증 | Windows와 Linux에서의 실행(링크, 터미널 동작, 종료 코드 전달) | 미검증 |
| 미검증 | zenity의 `Ctrl+H`, kdialog의 `Alt+.` 단축키 안내 문구의 정확성 | 문서 미확인 |

5. `수동 확인 절차`에 업데이트 확인 단계를 하위 절로 추가한다. 실제 캐시를 건드리지 않도록 임시 `HOME`을 쓰는 절차로 쓴다(macOS 기준 경로 `$HOME/Library/Caches/sshtui/update-check`, Linux는 `$HOME/.cache/sshtui/update-check`).
   - 알림: 캐시에 `checked_at=<현재 시각>`과 `latest=9.9.9`를 써 넣고 실행, 제목 줄 알림과 `u`의 Update 화면 확인
   - 백그라운드 확인: `checked_at=1`로 두고 실행 후 종료, `checked_at`이 현재 시각으로 바뀌었는지 확인
   - 비활성화: `SSHTUI_NO_UPDATE_CHECK=1`로 실행 후 캐시 무변화와 알림 없음 확인
   - 즉시 확인: `sshtui --check-update; echo $?`
   - 접속 불가: `PATH`에서 `curl`을 뺀 실행, `https_proxy`와 `HTTPS_PROXY`를 모두 `http://127.0.0.1:9`로 설정한 실행(curl은 소문자 변수를 우선 읽으므로 둘 다 설정)
   - 임시 `HOME`을 쓰면 설치 방식 판정도 그 `HOME` 기준이 된다는 점 한 줄
6. `개발`의 모듈 구성 표에 `src/update.rs`(새 버전 확인, 캐시, 설치 방식 판정, 업그레이드 명령) 추가.

## 완료 조건

- 목차의 앵커 링크가 모두 실제 제목과 일치(번호가 바뀐 절 포함)
- `·`, `①` 등 금지 기호 없음
- `git status`에 `README.md`만 변경으로 표시
