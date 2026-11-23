# 작업 38: 릴리스 워크플로의 Linux 링크 실패 수정 (cargo-zigbuild 고정 버전 변경)

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 배경

`release.yml`을 처음 실행한 결과 Windows 2개 작업은 성공했고 `linux` 작업이 실패했다.

```
error: linking with `.../cargo-zigbuild/0.22.3/wrappers/2303/zigcc-aarch64-unknown-linux-musl-5d3b.sh` failed: exit status: 1
  = note: error: unsupported linker arg: --fix-cortex-a53-843419
error: could not compile `sshtui` (bin "sshtui") due to 1 previous error
```

원인: CI의 Rust 1.100.0은 `aarch64-unknown-linux-musl`에서 `-Wl,--fix-cortex-a53-843419`를 링커에 넘기는데, 고정해 둔 `cargo-zigbuild 0.22.3`은 이 인자를 걸러 내지 않는다. `ci.yml`의 `package` 작업은 x86_64만 빌드해 이 문제가 드러나지 않았다.

검수자가 로컬(macOS, Rust 1.100.0, zig 0.16.0)에서 확인한 결과: `cargo-zigbuild 0.22.3`으로 같은 오류 재현, `cargo-zigbuild 0.24.1`로는 Linux 4개 대상 모두 빌드 성공(정적 링크, ARMv7 자산 `Tag_CPU_arch` v7, ARMv6 자산 v6).

## 대상 파일

- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`
- `README.md`(해당 버전을 적은 줄만)

## 기대 내용

1. 두 워크플로의 `pip install cargo-zigbuild==0.22.3 ziglang==0.16.0`을 `cargo-zigbuild==0.24.1 ziglang==0.16.0`으로 바꾼다. 다른 단계는 건드리지 않는다.
2. 같은 종류의 문제가 CI에서 먼저 드러나도록 `ci.yml`의 `package` 작업이 `aarch64-unknown-linux-musl`도 함께 빌드하게 한다(툴체인 대상 추가, `cargo zigbuild` 한 줄 추가, 패키징과 `install.sh` 실행 검증은 지금처럼 x86_64 기준). 기대 자산 확인 목록에 ARM64 tar.gz, `arm64.deb`와 별칭, `aarch64.rpm`과 별칭을 추가한다.
3. `README.md`의 `검증 상태` 표에서 로컬 릴리스 빌드 행의 `cargo-zigbuild 0.22.3`을 `cargo-zigbuild 0.24.1`로 고치고, 같은 행에 "Rust 1.100.0"을 추가한다. `8. 개발`에 고정 버전을 적은 곳이 있으면 함께 맞춘다. 다른 내용은 건드리지 않는다.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- 워크플로 YAML 구문 확인(가능한 도구로)
- `git status`에 대상 파일만 변경으로 표시
