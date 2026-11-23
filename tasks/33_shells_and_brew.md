# 작업 33: 더 많은 셸의 히스토리 지원과 Homebrew 업그레이드 명령 수정

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 목표

배포 준비에 맞춰 코드 두 곳을 고친다.

1. 셸 히스토리 후보가 zsh, bash, fish, PowerShell(Windows) 외의 셸에서도 나오도록 기본 히스토리 파일 목록을 넓힌다.
2. Homebrew로 설치한 경우의 업그레이드 명령을 전체 이름으로 바꾼다. Homebrew 7은 신뢰 등록되지 않은 탭의 포뮬러를 짧은 이름(`brew upgrade sshtui`)으로 지정하면 거부하고, 전체 이름(`<owner>/<tap>/<name>`)은 허용한다.

## 대상 파일

- `src/history.rs`
- `src/update.rs`
- `src/main.rs`(`--help` 문구가 달라질 때만)
- 위 변경으로 기대값이 달라지는 테스트가 있는 파일(`src/app/tests.rs`, `src/ui/tests.rs` 등)

## 1. 히스토리 파일 목록 (`src/history.rs`)

`history_sources`의 기본 목록(비활성화와 단일 파일 지정 규칙은 그대로)을 다음 순서로 한다. 중복 경로 제거와 `None` 생략 규칙은 지금과 같다.

| 순서 | 경로 | 형식 | 셸 |
|---|---|---|---|
| 1 | `HISTFILE`(비어 있지 않을 때) | `format_for_path` | 사용자 지정 |
| 2 | `$ZDOTDIR/.zsh_history` | Zsh | zsh |
| 3 | `<home>/.zsh_history` | Zsh | zsh |
| 4 | `<home>/.bash_history` | Plain | bash |
| 5 | `$XDG_DATA_HOME/fish/fish_history` 또는 `<home>/.local/share/fish/fish_history` | Fish | fish |
| 6 | `<home>/.history` | Plain | tcsh, csh |
| 7 | `<home>/.sh_history` | Plain | ksh |
| 8 | `<home>/.mksh_history` | Plain | mksh |
| 9 | `<config_dir>/nushell/history.txt` | Plain | nushell(텍스트 형식) |
| 10 | `<home>/.local/share/powershell/PSReadLine/ConsoleHost_history.txt` | Plain | PowerShell(macOS, Linux) |
| 11 | `<config_dir>/Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt` | Plain | PowerShell(Windows) |

`Plain` 형식의 해석 보완(`parse_commands`):

- tcsh의 타임스탬프 줄 `#+<숫자>`도 건너뛴다(지금의 `#<숫자>`에 더해).
- ksh93 히스토리는 명령 사이에 NUL 바이트가 들어 있다. NUL을 줄 구분으로 취급하고, 각 줄 앞의 제어 문자(0x00-0x1F, 0x7F와 lossy 변환으로 생긴 U+FFFD)를 떼어 낸다.
- 그 밖의 동작(빈 줄 제외, CRLF, 손실 변환)은 그대로.

nushell의 SQLite 형식(`history.sqlite3`)과 elvish, xonsh는 다루지 않는다.

테스트: 기본 목록과 순서(기존 기대값을 새 목록으로 갱신), `#+1700000000` 줄 건너뛰기, NUL로 구분된 명령과 앞 제어 문자 제거, `config_dir`이 `None`일 때 9번과 11번 생략, `read_ssh_commands`로 tcsh 형식과 ksh 형식 임시 파일에서 ssh 명령만 반환.

## 2. Homebrew 업그레이드 명령 (`src/update.rs`)

- 상수 추가: `pub const HOMEBREW_FORMULA: &str = "hungryZoo/tap/sshtui";`(문서 주석에 전체 이름을 쓰는 이유 한 줄).
- `InstallMethod::Brew`의 업그레이드 단계와 표시 문구를 `brew upgrade hungryZoo/tap/sshtui`로 바꾼다. 인자 배열은 `["upgrade", HOMEBREW_FORMULA]`.
- 다른 설치 방식의 단계와 자산 이름(`install.sh`, `sshtui_<arch>.deb`, `sshtui-<arch>.rpm`)은 바꾸지 않는다.
- 이 문구를 단언하는 기존 테스트의 기대값을 새 명령으로 고친다.

## 기존 테스트 수정 허용 범위

- `history_sources`의 기본 목록 기대값
- `brew upgrade sshtui`를 기대하던 단언

고친 테스트 이름을 최종 보고에 나열한다.

## 제약

- 실제 홈의 히스토리 파일 접근 금지. `Cargo.toml` 수정 금지.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- `git status`에 대상 파일만 변경으로 표시
