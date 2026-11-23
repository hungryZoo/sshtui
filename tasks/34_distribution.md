# 작업 34: 배포 구성 (설치 스크립트, 릴리스와 CI 워크플로, 패키징)

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 이 작업은 Rust 소스를 고치지 않고 배포에 필요한 파일을 만든다.

## 반드시 먼저 읽을 참조 자료

모두 저장소 안의 `logs/reference/`에 복사해 두었다(git에서 제외되는 폴더, 읽기만 한다).

- `logs/reference/skills/cross-platform-cli-distribution/SKILL.md`와 `reference/install.sh`: 이 작업의 기준 규칙. 1-5절의 규칙을 모두 적용한다.
- `logs/reference/skills/homebrew-install-docs/SKILL.md`: 포뮬러 구성과 전체 이름 사용
- `logs/reference/skills/release-notes/SKILL.md`와 `reference/`: 릴리스 노트 형식
- `logs/reference/tu/`: 같은 저장소 주인의 다른 프로젝트(`tu`)의 배포 파일. 파일 구성과 스크립트 작성 방식을 따르되, 스킬 문서와 다르면 스킬 문서가 우선한다(`tu`의 워크플로는 스킬보다 오래되었다).

## 배포 대상과 자산 이름

저장소는 `hungryZoo/sshtui`, 실행 파일과 패키지 이름은 `sshtui`, 버전은 `Cargo.toml`의 값.

| 자산 | 대상 | 빌드 |
|---|---|---|
| `sshtui-<ver>-aarch64-apple-darwin.tar.gz` | macOS Apple Silicon | macOS 러너, Apple 툴체인, `MACOSX_DEPLOYMENT_TARGET=11.0` |
| `sshtui-<ver>-x86_64-apple-darwin.tar.gz` | macOS Intel | 같은 러너에서 교차 빌드 |
| `sshtui-<ver>-x86_64-unknown-linux-musl.tar.gz` | Linux x86_64 (정적) | `cargo zigbuild` |
| `sshtui-<ver>-aarch64-unknown-linux-musl.tar.gz` | Linux ARM64 (정적) | `cargo zigbuild` |
| `sshtui-<ver>-armv7-unknown-linux-musleabihf.tar.gz` | Linux ARMv7 (정적) | `cargo zigbuild` |
| `sshtui-<ver>-arm-unknown-linux-musleabihf.tar.gz` | Linux ARMv6 hard-float (정적, Pi 1과 Zero) | `cargo zigbuild` |
| `sshtui-<ver>-x86_64-pc-windows-msvc.zip` | Windows x64 | `windows-2025` 러너 |
| `sshtui-<ver>-aarch64-pc-windows-msvc.zip` | Windows ARM64 | `windows-11-arm` 러너 |
| `sshtui_<ver>_amd64.deb`, `sshtui_<ver>_arm64.deb`, `sshtui_<ver>_armhf.deb` | Debian 계열 | musl 바이너리로 `cargo deb --no-build`. armhf는 ARMv6 바이너리 |
| `sshtui-<ver>-1.x86_64.rpm`, `sshtui-<ver>-1.aarch64.rpm` | RPM 계열 | `cargo generate-rpm` |
| `sshtui_amd64.deb`, `sshtui_arm64.deb`, `sshtui_armhf.deb`, `sshtui-x86_64.rpm`, `sshtui-aarch64.rpm` | 버전 없는 별칭 | 위 패키지의 복사본 |
| `install.sh`, `install.ps1`, `SHA256SUMS` | | |

- Linux 자산은 전부 musl 정적 링크다. glibc 자산은 만들지 않는다.
- tar.gz와 zip 안에는 실행 파일, `README.md`, `LICENSE`.
- 버전 없는 별칭의 이름은 `src/update.rs`가 쓰는 이름(`sshtui_<arch>.deb`, `sshtui-<arch>.rpm`)과 같아야 한다. 확인하고 맞춘다.

## 만들 파일

### 1. `LICENSE`

MIT. 저작권 표기 `Copyright (c) 2026 hungryZoo`(`logs/reference/tu/LICENSE`와 같은 형식).

### 2. `Cargo.toml`

의존성은 건드리지 않는다. 다음만 추가한다.

- `[package]`: `authors = ["hungryZoo"]`(이메일 주소를 넣지 않는다), `license = "MIT"`, `homepage`, `readme = "README.md"`, `keywords`, `categories = ["command-line-utilities"]`. `publish = false`는 유지.
- `[profile.release]`: `lto = true`, `codegen-units = 1`, `strip = true`. `panic = "abort"`는 넣지 않는다(패닉 시 터미널 복원 경로 유지).
- `[package.metadata.deb]`: `maintainer = "hungryZoo <hungryZoo@users.noreply.github.com>"`, `depends = "openssh-client"`, `section = "net"`, 자산은 실행 파일과 README와 LICENSE(`tu`와 같은 배치).
- `[package.metadata.generate-rpm]`: `license = "MIT"`, `auto-req = "disabled"`, 같은 자산, `[package.metadata.generate-rpm.requires]`에 `openssh-clients = "*"`.

`cargo metadata --offline`과 공통 완료 조건이 계속 통과해야 한다.

### 3. `.cargo/config.toml`

Windows MSVC에서 C 런타임 정적 링크(스킬 4절의 설정 그대로).

### 4. `install.sh` (저장소 루트)

`logs/reference/skills/cross-platform-cli-distribution/reference/install.sh`에서 시작해 스킬 1절의 규칙을 모두 적용한다. 추가 요구:

- `REPO="hungryZoo/sshtui"`. 환경변수 `SSHTUI_VERSION`, `SSHTUI_INSTALL_DIR`(별칭 `SSHTUI_BIN_DIR`), 옵션 `--version`, `--bin-dir`, `--help`.
- `SSHTUI_DOWNLOAD_BASE`: 자산을 받을 기준 URL(기본 `https://github.com/<repo>/releases/download/v<version>`). 미러와 로컬 검증용이며 `file://` URL도 동작해야 한다. 이 값이 있으면 버전은 `--version`이나 `SSHTUI_VERSION`으로 주어야 하고, 없으면 그 사실을 알리는 오류로 끝낸다.
- 대상 판별: macOS는 하드웨어 기준(`sysctl -n hw.optional.arm64`). Linux는 `x86_64`/`amd64`, `aarch64`/`arm64`(정적 바이너리이므로 32비트 사용자 공간에서도 aarch64 자산), `armv7l`/`armv8l`, `armv6l`. 그 밖의 아키텍처와 OS는 지원 목록과 소스 빌드 안내(`cargo install --git`)를 담은 오류. MSYS, MINGW, Cygwin에서는 `install.ps1` 사용 안내.
- 다운로드 도구: `curl` 우선, 없으면 `wget`. 최신 버전 확인은 `curl`이면 `/releases/latest` 리디렉션 우선과 API 대체, `wget`이면 API.
- `SHA256SUMS`를 받아 `sha256sum`, `shasum -a 256`, `openssl dgst -sha256` 가운데 있는 것으로 검증. 불일치, 해당 줄 없음, 검증 도구 없음은 모두 중단. 실패 시 기존 바이너리를 건드리지 않는다.
- 설치는 같은 디렉터리의 임시 파일에 쓴 뒤 `mv`로 교체(`install` 명령에 의존하지 않음). 설치 후 `<bin_dir>/sshtui --version`의 결과를 설치하려던 버전과 비교.
- `ssh`가 PATH에 없으면 경고만 출력.
- PATH 안내: 설치 디렉터리가 PATH에 없으면 `$SHELL`의 이름에 맞는 한 줄을 출력한다. fish는 `fish_add_path <dir>`, csh와 tcsh는 `set path = (<dir> $path)`, nu는 `$env.PATH = ($env.PATH | prepend "<dir>")`, 그 외(sh, bash, zsh, ksh, dash 등)는 `export PATH="<dir>:$PATH"`. 셸 설정 파일은 절대 수정하지 않는다.
- POSIX `sh`, `set -eu`, bash 전용 문법 금지. `dash`와 busybox `sh`에서 동작해야 한다.

### 5. `install.ps1` (저장소 루트)

스킬 5절의 규칙을 모두 적용한다. 추가 요구:

- 환경변수 `SSHTUI_VERSION`, `SSHTUI_INSTALL_DIR`(기본 `%LOCALAPPDATA%\Programs\sshtui\bin`), `SSHTUI_ARCH`(판별 무시), `SSHTUI_NO_MODIFY_PATH`, `SSHTUI_DOWNLOAD_BASE`.
- 자산은 `sshtui-<ver>-<x86_64|aarch64>-pc-windows-msvc.zip`. `SHA256SUMS` 검증 필수.
- 기존 `sshtui.exe`가 실행 중이라 덮어쓸 수 없으면 `sshtui.exe.old`로 이름을 바꾼 뒤 새 파일을 놓고, 남아 있는 `.old` 파일은 다음 설치 때 지운다.
- 설치 후 `sshtui.exe --version` 확인. `ssh.exe`가 없으면 OpenSSH 클라이언트 설치 안내 경고.
- 파일 전체가 ASCII여야 한다.

### 6. `scripts/`

`tu`의 구성을 따른다(`bash`, `set -euo pipefail`). Python을 쓰지 않는다.

- `meta.sh`: `Cargo.toml`에서 `BIN`(패키지 이름)과 `VERSION`을 읽는다.
- `build-all.sh`: 개발 장비(macOS)에서 macOS 2개 대상은 `cargo build`, Linux 4개 대상은 `cargo zigbuild`로 빌드. `MACOSX_DEPLOYMENT_TARGET=11.0`.
- `package-all.sh`: `target/<triple>/release/`에 있는 바이너리만 골라 위 표의 자산을 `dist/`에 만든다(없는 대상은 건너뜀). Windows `.exe`가 있으면 zip도 만든다. 버전 없는 별칭, `install.sh`, `install.ps1` 복사, `SHA256SUMS`(`dist/`의 모든 파일, `sha256sum`이 없으면 `shasum -a 256`). `--checksums-only` 옵션: 패키징 없이 `dist/`의 현재 파일로 `SHA256SUMS`만 다시 만든다(릴리스 워크플로가 여러 러너의 결과를 모은 뒤 사용).
- `update-formula.sh VERSION SHA256SUMS FORMULA_PATH`: Homebrew 포뮬러 파일 전체를 새로 써서 만든다(파일이 없어도 생성). 클래스 `Sshtui`, `desc`, `homepage`, `version`, `license "MIT"`, `on_macos`와 `on_linux` 안에 `on_arm`과 `on_intel` 블록 4개(Linux는 musl 자산), `bin.install "sshtui"`, `test do`에서 `--version` 확인. `SHA256SUMS`에 필요한 줄이 없으면 실패.
- `post-release.sh [VERSION]`: 릴리스 뒤 개발 장비에서 실행하는 수동 절차. 로컬 `gh` 로그인으로 `SHA256SUMS`를 받아 `hungryZoo/homebrew-tap`의 `Formula/sshtui.rb`를 갱신하고 푸시. crates.io 게시는 하지 않는다.

`.gitignore`에 `/dist` 추가.

### 7. `.github/workflows/ci.yml`

푸시와 PR에서 실행.

- `check`: `ubuntu-latest`, `macos-latest`, `windows-latest` 매트릭스에서 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- `scripts`: `shellcheck install.sh scripts/*.sh`, `install.ps1`의 ASCII 검사와 PowerShell 구문 분석(`pwsh`).
- `package`: Linux에서 `x86_64-unknown-linux-musl`을 `cargo zigbuild`로 빌드해 `scripts/package-all.sh` 실행, 기대 자산 이름 확인, `SSHTUI_DOWNLOAD_BASE=file://$PWD/dist`로 `install.sh`를 임시 디렉터리에 실제로 실행해 `--version` 확인.

### 8. `.github/workflows/release.yml`

`v*` 태그 푸시와 수동 실행(`workflow_dispatch`)으로 시작. `tu`의 버전과 태그 처리 방식(태그와 `Cargo.toml` 버전 일치 확인, 빌드와 패키징이 끝난 뒤에만 태그 생성)을 따르고, 다음을 스킬에 맞춘다.

- 작업 분리: `linux`(ubuntu, zigbuild 4개 대상과 deb, rpm), `macos`(`macos-latest`, 2개 대상), `windows`(매트릭스 `windows-2025`와 `windows-11-arm`), `publish`(결과를 모아 릴리스).
- 도구 버전 고정: `pip install cargo-zigbuild==0.22.3 ziglang==0.16.0`.
- 올리기 전 바이너리 검사(실패하면 워크플로 실패):
  - Linux: 각 바이너리가 정적 링크인지(`readelf -d`에 `NEEDED` 없음 또는 `file` 출력), ARMv6 자산의 `Tag_CPU_arch`가 v6, ARMv7 자산이 v7인지(`readelf -A`)
  - macOS: `otool -l`의 `minos`가 11.0, arm64 바이너리의 `codesign -v`
  - 실행 가능한 러너의 바이너리는 `--version`이 `sshtui <ver>`
- 릴리스 무결성: 초안(`--draft`)으로 만들고 모든 자산을 올린 뒤 마지막에 공개. `actions/attest-build-provenance`로 `SHA256SUMS` 기준 증명 생성(필요한 `permissions` 지정).
- 릴리스 제목과 본문은 `.github/release-notes/<tag>.md`가 있으면 그것(첫 `#` 제목이 릴리스 제목), 없으면 자동 생성.
- Homebrew 탭 갱신 단계: 비밀 값 `HOMEBREW_TAP_TOKEN`이 없으면 경고만 내고 건너뜀(`tu`와 같은 방식).
- 기대 자산 목록을 워크플로 안에서 확인하고 하나라도 없으면 공개하지 않는다.

### 9. `.github/release-notes/v0.1.0.md`

release-notes 스킬의 구조로 작성(영어). 첫 릴리스이므로 "What's new since" 대신 "What's in this release"로 기능을 사용자 관점에서 나열하고, 설치 한 줄(`curl ... | sh`, PowerShell 한 줄), 버전 고정 방법, 다른 자산과 체크섬 검증 명령을 넣는다. 기능 목록은 `README.md`와 소스에서 확인한 것만 쓴다. 검증하지 않은 플랫폼(Windows와 Linux에서의 실행)은 "built but not yet tested on real hardware"로 명시한다.

## 제약

- `src/`, `tasks/`, `logs/`, `AGENTS.md`, `README.md` 수정 금지(README는 다음 작업).
- 네트워크가 없으므로 원격 저장소 생성, 푸시, 릴리스, 다운로드를 시도하지 않는다. 워크플로는 실행하지 않고 파일만 만든다.
- 스크립트와 워크플로에 비밀 값이나 이메일 주소를 넣지 않는다.
- 실제 `~/.ssh`와 홈 디렉터리를 건드리는 검증 금지. 스크립트를 시험하려면 임시 디렉터리만 쓴다.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- `sh -n install.sh`, `dash -n install.sh`(있으면), `bash -n scripts/*.sh` 통과
- `install.ps1`에 ASCII가 아닌 바이트 없음(`LC_ALL=C grep -n '[^ -~]' install.ps1`의 결과가 탭과 줄 끝뿐인지 확인)
- 가능하면 로컬에서 현재 호스트 대상만 빌드해 `scripts/package-all.sh`와 `install.sh`(`SSHTUI_DOWNLOAD_BASE=file://...`, 임시 설치 디렉터리)를 실제로 실행하고 결과를 최종 보고에 적는다. 샌드박스 제약으로 못 한 검증은 "미검증"으로 적는다.
- 최종 보고에 스킬의 규칙별 적용 여부를 표로 정리하고, 적용하지 못한 규칙과 이유를 적는다.
