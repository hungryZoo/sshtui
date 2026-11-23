# 작업 24: 셸 히스토리의 ssh 명령 추출과 마지막 접속 기록

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 목표

화면과 무관한 두 모듈을 만든다. 이 작업에서는 `Backend`, 앱 상태, 화면을 건드리지 않는다(뒤 작업에서 연결).

- `src/history.rs`: 셸 히스토리 파일에서 `ssh` 명령을 읽어, config에 아직 없는 접속 대상을 후보로 만든다.
- `src/state.rs`: 마지막으로 접속한 별칭을 파일 하나에 기록하고 읽는다.

## 대상 파일

- `src/history.rs` (신규), `src/state.rs` (신규)
- `src/lib.rs` (`pub mod history;`, `pub mod state;` 추가)

`Cargo.toml` 수정 금지. 기존 의존성(`dirs` 포함)만 사용한다.

## 1. `src/history.rs`

### 형식과 명령 추출

```rust
/// Format of a shell history file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFormat {
    /// zsh, plain or extended (`: <start>:<elapsed>;<command>`), with metafied bytes.
    Zsh,
    /// One command per line (bash, PowerShell). Lines of `#<digits>` are timestamps.
    Plain,
    /// fish (`- cmd: <command>` entries).
    Fish,
}

/// Returns the commands in file order (oldest first).
pub fn parse_commands(format: HistoryFormat, bytes: &[u8]) -> Vec<String>;
```

- 공통: 줄 끝의 `\r` 제거, 빈 명령 제외, UTF-8이 아닌 바이트는 손실 변환(패닉 없음).
- `Zsh`: 먼저 메타 해제(바이트 `0x83` 다음 바이트는 `^ 0x20`한 한 바이트로 치환). 줄이 `: <숫자>:<숫자>;`로 시작하면 그 뒤가 명령. 줄이 `\`로 끝나면 다음 줄과 이어진 한 명령(`\`를 떼고 `\n`으로 연결).
- `Plain`: `#` 뒤가 숫자뿐인 줄은 건너뜀.
- `Fish`: `- cmd: `로 시작하는 줄의 나머지가 명령. `\\`는 `\`, `\n`은 줄바꿈으로 해제. 다른 줄은 무시.

### ssh 명령 해석

```rust
/// Connection settings taken from one ssh command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshInvocation {
    pub user: Option<String>,
    pub address: String,
    /// None for the default port 22.
    pub port: Option<String>,
    pub key_file: Option<String>,
}

/// Parses a simple `ssh ...` command. Returns None for anything it cannot represent safely.
pub fn parse_ssh_command(command: &str) -> Option<SshInvocation>;
```

토큰 분리:

- 공백과 탭으로 분리. 작은따옴표 안은 그대로, 큰따옴표 안은 `\"`와 `\\`만 해제, 따옴표 밖의 `\`는 다음 글자를 그대로 취함. 닫히지 않은 따옴표는 `None`.
- 작은따옴표 밖에 `` ` ``, `$`, `|`, `&`, `;`, `<`, `>`, `(`, `)`, `{`, `}`, `*`, `?`, `#`, 줄바꿈이 있으면 `None`(단순 명령만 다룸). `~`는 허용.
- 첫 토큰이 정확히 `ssh`가 아니면 `None`(`sudo ssh`, `VAR=x ssh`, `/usr/bin/ssh` 모두 `None`).

옵션(OpenSSH의 getopt 규칙):

- 인자 없는 플래그: `46AaCfgKkMNnqsTtvXxYy`. 한 토큰에 여러 개를 붙여 쓸 수 있음(`-vvA`).
- 인자를 받는 옵션: `BbcDEeFIiJLlmOoPpQRSWw`. 인자는 같은 토큰의 나머지(`-p2222`) 또는 다음 토큰. 인자가 없으면 `None`.
- `--`는 옵션의 끝. 표에 없는 옵션 글자는 `None`.
- `-p`: 포트. `-l`: 사용자. `-i`: 키 파일(처음 것만 사용).
- `-o`: `키=값` 또는 `키 값`(첫 `=`이나 공백에서 분리, 키는 대소문자 무시). `Port`, `User`, `IdentityFile`, `HostName`은 값으로 사용(`HostName`은 주소를 대체). `ProxyJump`, `ProxyCommand`가 있으면 `None`. 그 밖의 키는 무시.
- `-F`, `-J`, `-W`, `-G`, `-Q`, `-V`, `-O`가 있으면 `None`(저장해도 같은 접속이 되지 않거나 접속이 아님).
- 나머지 인자 옵션(`-L`, `-R`, `-D` 등)은 값을 읽고 버림.

대상:

- 옵션이 아닌 첫 토큰이 대상. 없으면 `None`. 그 뒤의 토큰(원격 명령)은 무시.
- `user@host`: 마지막 `@`에서 분리. `ssh://[user@]host[:port]`도 지원하고 `[::1]` 형태의 대괄호는 벗김. `ssh://` 뒤에 `/`로 시작하는 경로가 있으면 버림.
- 우선순위(ssh와 동일): 사용자는 `-l`과 `-o User` 가운데 명령에 먼저 나온 것, 없으면 `user@`. 포트는 `-p`와 `-o Port` 가운데 먼저 나온 것, 없으면 URI의 포트.
- 포트가 22이면 `port`는 `None`.
- `crate::validate`의 `validate_address`, `validate_user`, `parse_port`, `validate_key_file`로 검증하고 하나라도 실패하면 `None`.

### 후보 만들기

```rust
/// A connection found in history that is not saved yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub invocation: SshInvocation,
    /// Number of history commands that resolve to this connection.
    pub count: usize,
}

/// A saved host used to skip destinations that are already configured.
#[derive(Debug, Clone, Copy)]
pub struct SavedHost<'a> {
    pub alias: &'a str,
    pub host_name: Option<&'a str>,
}

/// Upper bound on the number of suggestions.
pub const MAX_SUGGESTIONS: usize = 20;

/// Builds suggestions from commands given oldest first. The result is newest first.
pub fn suggest<'a>(
    commands: impl IntoIterator<Item = &'a str>,
    saved: &[SavedHost<'_>],
) -> Vec<Suggestion>;
```

- 같은 접속의 기준: `user`, 주소(ASCII 대소문자 무시), `port`가 같음. 합칠 때 `count`를 더하고, `key_file`은 키 파일이 있었던 가장 최근 명령의 값, 주소 표기는 가장 최근 명령의 값.
- 순서: 마지막으로 등장한 시점이 최근인 것부터.
- 제외: 주소가 `saved`의 어떤 `alias` 또는 `host_name`과 같으면(ASCII 대소문자 무시) 제외.
- `MAX_SUGGESTIONS`개까지만 반환.

### 히스토리 파일 위치와 읽기

```rust
/// Environment variable that replaces the default history files with one file.
pub const HISTORY_FILE_ENV: &str = "SSHTUI_HISTORY_FILE";
/// Environment variable that disables reading shell history when nonempty and not "0".
pub const DISABLE_ENV: &str = "SSHTUI_NO_HISTORY";
/// Only this many bytes at the end of each file are read.
pub const MAX_READ_BYTES: u64 = 256 * 1024;

/// A history file and how to parse it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySource {
    pub path: PathBuf,
    pub format: HistoryFormat,
}

/// Chooses the format from the file name: contains "fish" -> Fish, contains "zsh" -> Zsh, otherwise Plain.
pub fn format_for_path(path: &Path) -> HistoryFormat;

/// Lists candidate history files. Pure: every lookup is injected.
pub fn history_sources(
    home: Option<&Path>,
    config_dir: Option<&Path>,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Vec<HistorySource>;

/// Reads the tail of each existing source and returns only `ssh` commands, oldest first per file,
/// files in the given order.
pub fn read_ssh_commands(sources: &[HistorySource]) -> Vec<String>;
```

`history_sources`:

- `DISABLE_ENV`가 비어 있지 않고 `0`이 아니면 빈 목록.
- `HISTORY_FILE_ENV`가 비어 있지 않으면 그 경로 하나만(형식은 `format_for_path`).
- 그 외에는 다음 순서, 중복 경로 제거: `HISTFILE`(비어 있지 않을 때, 형식은 `format_for_path`), `$ZDOTDIR/.zsh_history`(`ZDOTDIR`이 있을 때), `<home>/.zsh_history`, `<home>/.bash_history`, `$XDG_DATA_HOME/fish/fish_history`(없으면 `<home>/.local/share/fish/fish_history`), `<config_dir>/Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt`(Plain). `home`이나 `config_dir`이 `None`이면 해당 항목 생략.

`read_ssh_commands`:

- 파일을 읽기 전용으로 열고 끝에서 `MAX_READ_BYTES`만 읽는다. 중간부터 읽었으면 첫 줄바꿈까지의 조각을 버린다.
- 없는 파일, 읽을 수 없는 파일, 디렉터리는 조용히 건너뜀. 파일을 쓰거나 만들지 않는다.
- `parse_commands` 결과 가운데 앞 공백을 뗀 뒤 `ssh` 다음이 공백이나 탭인 명령만 반환. ssh가 아닌 히스토리 내용은 이 함수 밖으로 나가지 않는다.
- 히스토리 내용을 출력하거나 로그에 남기지 않는다.

### 필수 테스트

- `parse_commands`: zsh 확장 형식과 일반 형식 혼합, `\`로 이어진 여러 줄 명령, 메타 처리된 바이트(`0x83 0xA3`이 `0x83` 한 바이트가 되는 예), bash 타임스탬프 줄, fish의 `- cmd:`와 `when:` 줄과 이스케이프, CRLF, UTF-8이 아닌 바이트
- `parse_ssh_command` 성공: `ssh host`, `ssh user@host`, `ssh -p 2222 user@host`, `ssh -p2222 host`, `ssh -vvA -i ~/.ssh/id_ed25519 host`, `ssh -l root host`(사용자 우선순위: `ssh -l a b@host` → `a`), `ssh -o Port=2200 -o User=deploy host`, `ssh -o "IdentityFile ~/.ssh/k" host`, `ssh -o HostName=10.0.0.5 alias`, `ssh host uptime`, `ssh -L 8080:localhost:80 host`, `ssh ssh://user@host:2222`, `ssh ssh://[::1]:2222`, `ssh -p 22 host`(port `None`), `ssh 'host'`, `ssh -- host`
- `parse_ssh_command` 실패(`None`): `sudo ssh host`, `ssh`, `ssh -p`, `ssh -J jump host`, `ssh -F cfg host`, `ssh -o ProxyJump=j host`, `ssh -Z host`, `ssh $HOST`, `ssh host | tee log`, `ssh host; ls`, `` ssh `cat h` ``, `ssh "host`, `ssh -p 0 host`, `ssh -p abc host`, `ssh -- -host`, `ssh -G host`, `scp a host:b`, `sshfs host: /mnt`
- `suggest`: 중복 합산과 최신순, 최근 명령의 키 파일 유지, alias와 host_name 일치 제외(대소문자 무시), 사용자나 포트가 다르면 별도 후보, 21개 이상일 때 20개
- `format_for_path`, `history_sources`: 비활성화, 단일 파일 지정, 기본 목록과 순서, `HISTFILE` 중복 제거, `ZDOTDIR`, `XDG_DATA_HOME`, `home`이 `None`
- `read_ssh_commands`(`tempfile`): ssh 명령만 반환, 없는 파일과 디렉터리 건너뜀, `MAX_READ_BYTES`보다 큰 파일에서 끝부분만 읽고 첫 조각을 버림, 파일이 수정되지 않음

## 2. `src/state.rs`

```rust
/// Returns the file that stores the last connected alias, if a per-user state directory exists.
/// `<state dir or local data dir>/sshtui/last-host`
pub fn default_state_file() -> Option<PathBuf>;

/// Reads the last connected alias. None when missing, unreadable, empty or malformed.
pub fn read_last_host(path: &Path) -> Option<String>;

/// Records the last connected alias, creating parent directories as needed.
pub fn write_last_host(path: &Path, alias: &str) -> io::Result<()>;
```

- `default_state_file`: `dirs::state_dir()`가 있으면 그것, 없으면 `dirs::data_local_dir()`. 이 함수는 경로만 만들고 파일 시스템에 접근하지 않는다. 테스트에서 호출하지 않는다.
- `read_last_host`: 파일의 처음 256바이트까지만 읽어 첫 줄을 사용. 앞뒤 공백 제거 후 비었거나, 제어 문자가 있거나, UTF-8이 아니면 `None`.
- `write_last_host`: 내용은 `<alias>\n`. 별칭이 비었거나 제어 문자를 포함하면 `io::ErrorKind::InvalidInput`.

테스트(`tempfile`): 쓰고 읽기, 상위 디렉터리 자동 생성, 덮어쓰기, 없는 파일, 빈 파일, 제어 문자, 256바이트 초과 내용, 잘못된 별칭 거부.

## 제약

- 실제 홈 디렉터리의 히스토리나 상태 파일을 읽거나 쓰는 테스트 금지. 모든 경로는 인자로 주입한다.
- 라이브러리 코드에서 `unwrap`, `expect`, `panic!` 금지. 공개 항목에 영어 문서 주석.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- 기존 테스트 변경 없음
- `git status`에 대상 파일만 변경으로 표시
