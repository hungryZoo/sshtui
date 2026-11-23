# 작업 39: README에 첫 릴리스와 CI 결과 반영

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `tasks/11_readme.md`의 작성 원칙(한국어, 명사형 개조식, 비유 금지, 원 숫자 기호 금지, 범위는 `-`, 앵커 일치)을 적용한다. 코드는 수정하지 않는다.

## 목표

저장소 공개와 `v0.1.0` 릴리스 게시 뒤의 사실로 `README.md`의 검증 상태와 지원 플랫폼의 확인 수준, 릴리스 절차를 갱신한다. 아래 "검수자 확인 내용"만 근거로 쓰고, 여기에 없는 것을 확인된 것처럼 쓰지 않는다.

## 대상 파일

- `README.md`

## 검수자 확인 내용 (2026-11-24, `v0.1.0`)

GitHub Actions(`ci.yml`, 커밋 `361fd24`):

- `check`: `ubuntu-latest`, `macos-latest`, `windows-latest`에서 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 통과(Rust 1.100.0)
- `scripts`: `shellcheck install.sh scripts/*.sh`, `sh -n`과 `dash -n`, `install.ps1`의 ASCII 검사와 PowerShell 구문 분석 통과
- `package`: `x86_64-unknown-linux-musl`과 `aarch64-unknown-linux-musl`을 `cargo zigbuild`로 빌드, 패키징, 자산 이름 확인, Linux 러너에서 `install.sh`(`file://` 기준 URL) 실행과 `--version` 확인 통과

GitHub Actions(`release.yml`, 수동 실행):

- `linux`: 4개 대상 빌드, 정적 링크 검사(`readelf`), ARMv7 자산 `Tag_CPU_arch` v7과 ARMv6 자산 v6 검사, x86_64 바이너리의 `--version` 실행, deb와 rpm 패키징 통과
- `macos`: 2개 대상 빌드, `minos` 11.0 검사, arm64 `codesign -v`, `--version` 실행 통과
- `windows`: `windows-2025`에서 x64, `windows-11-arm`에서 ARM64를 각각 빌드하고 그 러너에서 `sshtui.exe --version` 실행 통과
- `publish`: 자산 21개 목록 확인, `SHA256SUMS` 검증, 빌드 출처 증명 생성, 초안으로 올린 뒤 공개

게시된 릴리스를 macOS(Apple Silicon)에서 확인:

- 자산 21개를 내려받아 `SHA256SUMS`의 20개 항목 검증 통과
- tar.gz 6개의 내용물(실행 파일, `README.md`, `LICENSE`)과 바이너리 형식 확인. Linux 4개는 정적 링크, macOS 2개는 `minos` 11.0, arm64는 `codesign -v` 통과. arm64 바이너리는 직접, x86_64 바이너리는 Rosetta에서 `--version` 실행
- zip 2개의 내용물(`sshtui.exe`, `README.md`, `LICENSE`) 확인
- `gh attestation verify -R hungryZoo/sshtui <tar.gz>` 종료 코드 0
- 임시 `HOME`에서 `curl -fsSL https://raw.githubusercontent.com/hungryZoo/sshtui/main/install.sh | sh` 실행: 최신 버전 확인, 다운로드, 체크섬 검증, `~/.local/bin` 설치, `--version` 일치
- `https://github.com/hungryZoo/sshtui/releases/latest/download/sshtui_amd64.deb`가 200으로 연결
- 로컬 빌드의 `sshtui --check-update`가 `sshtui 0.1.0 is up to date (latest release: 0.1.0)`와 종료 코드 0

Homebrew:

- `scripts/post-release.sh`로 `hungryZoo/homebrew-tap`에 `Formula/sshtui.rb` 푸시
- 포뮬러의 주소 4개가 200이고 `sha256`이 릴리스의 `SHA256SUMS`와 일치, `brew style` 지적 없음, `brew install --dry-run hungryZoo/tap/sshtui`가 `Would install 1 formula` 출력
- 실제 `brew install`은 실행하지 않음

여전히 미검증:

- Windows: `install.ps1`의 실제 실행과 PATH 갱신, TUI의 대화형 실행, 네이티브 파일 다이얼로그
- Linux: TUI의 대화형 실행, `.deb`와 `.rpm` 설치, ARM 실기기(Raspberry Pi 포함)
- 실제 `brew install`, 실제 서버에 대한 키 설치와 접속(기존 미검증 항목은 그대로)

운영 메모:

- macOS arm64 호스팅 러너가 15분 안에 배정되지 않아 작업이 취소되는 일이 있었다(`The job was not acquired by Runner of type hosted even after multiple attempts`). 실패한 작업만 재실행(`gh run rerun <run-id> --failed`)하면 앞서 성공한 작업의 결과물을 재사용해 이어진다.
- `cargo-zigbuild`는 0.24.1로 고정. 0.22.3은 Rust 1.100.0의 `aarch64-unknown-linux-musl` 링크에서 `unsupported linker arg: --fix-cortex-a53-843419`로 실패.
- 저장소의 immutable releases 설정은 꺼져 있다.

## 변경 내용

1. `6. 지원 플랫폼`의 `릴리스 대상` 표의 확인 수준 열을 위 내용으로 갱신한다. 값은 다음으로 한다.
   - `aarch64-apple-darwin`: 릴리스 자산 실행 확인 (macOS)
   - `x86_64-apple-darwin`: 릴리스 자산을 Rosetta에서 실행 확인
   - `x86_64-unknown-linux-musl`: CI 러너에서 `--version`과 설치 스크립트 실행 확인
   - `aarch64-unknown-linux-musl`, `armv7-...`, `arm-...`: CI에서 빌드와 정적 링크, CPU 속성 확인. 실기기 실행 미검증
   - Windows 2개: CI 러너에서 빌드와 `--version` 실행 확인. 대화형 실행과 `install.ps1` 미검증
   같은 절의 OS별 하위 절(macOS, Linux, Windows, Debian과 Ubuntu, Fedora와 RHEL 계열, Raspberry Pi, 지원하는 셸)의 확인 수준 서술도 맞춘다.
2. `7. 검증 상태`의 표: 자동 행에 GitHub Actions 결과를 추가하고(단위 테스트 수는 `cargo test --offline`의 실제 결과), 수동 행에 "게시된 릴리스 확인"과 "Homebrew 포뮬러 확인"을 추가한다. 미검증 행에서 이제 확인된 항목(GitHub Actions 미실행, Windows 릴리스 빌드, `shellcheck` 미실행, 실제 릴리스에서의 `curl ... | sh`, `releases/latest` 경로, 증명 검증)을 지우거나 남은 범위로 줄인다. 수동 행의 상태 표기는 `확인 (macOS)`, CI 결과는 `통과 (GitHub Actions)`.
3. `8. 개발`의 `릴리스 만드는 방법`: 실제로 수행한 순서(버전과 릴리스 노트 파일 준비, `main` 푸시와 CI 통과 확인, `release.yml` 수동 실행 또는 태그 푸시, 게시 확인, `scripts/post-release.sh`)로 정리하고 위 운영 메모 3개를 넣는다. 워크플로 파일과 대조해 틀린 서술이 없게 한다.
4. `2. 설치`와 머리 부분은 건드리지 않는다(이미 게시 후 기준).

## 완료 조건

- 목차와 내부 링크, 배지 링크의 앵커가 모두 실제 제목과 일치
- 원 숫자 기호 없음, `PTY` 문자열 없음, 버전 숫자가 직접 들어간 설치 명령 없음(검증 기록의 `v0.1.0` 언급은 허용)
- `git status`에 `README.md`만 변경으로 표시
