# 작업 34a: 배포 구성의 두 가지 수정

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 작업 34의 결과에서 아래 두 곳만 고친다.

## 대상 파일

- `Cargo.toml`
- `install.ps1`

## 1. `.deb`의 설명에 README 전체가 들어감

문제 위치: 검수자가 `scripts/package-all.sh`로 만든 `dist/sshtui_armhf.deb`의 `control` 파일에서 `Description`의 확장 설명이 `README.md`의 내용(` <div align="center">`로 시작)으로 채워졌다. `Cargo.toml`에 `readme = "README.md"`가 있고 `[package.metadata.deb]`에 `extended-description`이 없으면 `cargo-deb`가 README를 확장 설명으로 쓴다.

기대 동작: `[package.metadata.deb]`에 `extended-description`을 추가해 README가 들어가지 않게 한다. 내용은 영어 2-3줄(시스템 `ssh`를 실행하는 TUI이고, 호스트를 `~/.ssh/config`에 저장하며, SSH 프로토콜을 구현하지 않는다는 점). `[package.metadata.generate-rpm]`에도 같은 취지의 `summary` 한 줄을 추가한다. 다른 메타데이터와 의존성은 건드리지 않는다.

가능하면 로컬에서 호스트 대상 바이너리로 `cargo deb`를 다시 실행해 `control`의 `Description`을 확인하고 결과를 최종 보고에 적는다(`dist/`에 만든 파일은 git에 포함되지 않는다).

## 2. `install.ps1`의 `.old` 파일 삭제 실패 처리

문제 위치: `[IO.File]::Delete($old)`는 이전 업그레이드에서 남은 `sshtui.exe.old`가 아직 실행 중인 프로세스에 잡혀 있으면 예외를 던지고 설치 전체가 실패한다.

기대 동작: 남은 `.old` 파일의 삭제 실패(`IOException`, `UnauthorizedAccessException`)는 무시하고 설치를 계속한다. 그 경우 실행 중인 `sshtui.exe`를 옮길 이름이 겹치지 않도록, `.old`가 남아 있으면 고유한 이름(`sshtui.exe.<GUID>.old` 등)으로 옮기고, 설치 디렉터리에 남은 `sshtui.exe*.old` 파일은 다음 설치 때 지울 수 있는 것만 지운다. 파일은 계속 ASCII만 포함해야 한다.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과, `cargo metadata --offline` 통과
- `install.ps1`에 ASCII가 아닌 바이트 없음
- `git status`에 대상 파일만 변경으로 표시(이미 미추적 상태인 작업 34의 파일은 그대로)
