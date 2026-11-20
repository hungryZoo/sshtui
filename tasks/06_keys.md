# 작업 6: 비밀키 탐색

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 목표

ssh 디렉터리 직속의 비밀키 파일을 찾아 목록을 만든다. 키 내용은 읽지 않는다(첫 줄만 확인).

## 대상 파일

- `src/keys.rs`

## API

```rust
/// Type and comment taken from a `.pub` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKeyInfo {
    /// Key type, e.g. `ssh-ed25519`.
    pub key_type: String,
    /// Comment (rest of the line after the base64 blob), if any.
    pub comment: Option<String>,
}

/// A private key file found directly inside the ssh directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    pub path: PathBuf,
    /// File name (lossy UTF-8).
    pub file_name: String,
    /// Information from `<file_name>.pub` when that file exists and parses.
    pub public: Option<PublicKeyInfo>,
}
impl KeyInfo {
    /// One-line label for lists: `id_ed25519 (ssh-ed25519, user@host)`, `id_rsa (ssh-rsa)`,
    /// or just `id_x` without a `.pub` file.
    pub fn label(&self) -> String;
}

/// Pure: true when `line` (a first line, with or without its terminator) starts with
/// `-----BEGIN` and, after removing trailing `\r`, `\n`, spaces and tabs, ends with `PRIVATE KEY-----`.
pub fn is_private_key_first_line(line: &[u8]) -> bool;

/// Pure: parses the first line of a `.pub` file: `<type> <base64> [comment...]`.
/// Returns None for an empty line, a line with fewer than two fields, or a type containing
/// characters other than ASCII alphanumerics, `-`, `.`, `@` and `_`.
pub fn parse_public_key_line(line: &str) -> Option<PublicKeyInfo>;

/// Lists private keys directly inside `ssh_dir`, sorted by file name.
/// A missing directory yields an empty list. Other directory read errors are returned.
pub fn discover_keys(ssh_dir: &Path) -> std::io::Result<Vec<KeyInfo>>;
```

`discover_keys` 규칙:

- `ssh_dir`의 직속 항목만 본다. 하위 디렉터리는 내려가지 않는다.
- 일반 파일만 대상이다(`std::fs::metadata` 기준이므로 일반 파일을 가리키는 심볼릭 링크는 포함, 디렉터리와 소켓 등은 제외).
- 파일마다 앞에서부터 최대 256바이트 안에서 첫 줄만 읽는다(`Read::take(256)`과 `BufRead::read_until(b'\n')`). 256바이트 안에 줄바꿈이 없으면 읽은 만큼을 첫 줄로 본다. 그 이후의 내용은 읽지 않는다.
- 열 수 없거나 읽을 수 없는 파일은 조용히 건너뛴다.
- `.pub` 정보: `<경로>.pub`(파일명 뒤에 `.pub`를 붙인 경로)가 일반 파일이면 앞에서부터 최대 16384바이트 안에서 첫 줄만 읽어 `parse_public_key_line`에 넘긴다. UTF-8이 아니면 `public`은 `None`.
- `path`는 `ssh_dir.join(file_name)` 형태 그대로 둔다(canonicalize 금지).
- 키 파일의 내용(첫 줄 포함)을 반환값, 로그, 오류 메시지, 표준 출력 어디에도 넣지 않는다.

## 테스트

테스트 픽스처는 가짜 내용만 쓴다. 실제 키를 만들거나(`ssh-keygen` 실행 금지) 실제 키 내용을 넣지 않는다. 본문은 `AAAA` 같은 더미 문자열로 한다. `tempfile::tempdir()` 안에서만 수행한다.

`is_private_key_first_line`:

1. 통과: `-----BEGIN OPENSSH PRIVATE KEY-----`, `-----BEGIN RSA PRIVATE KEY-----`, `-----BEGIN EC PRIVATE KEY-----`, `-----BEGIN ENCRYPTED PRIVATE KEY-----`, `-----BEGIN PRIVATE KEY-----`, 끝에 `\n`, `\r\n`, 공백이 붙은 경우
2. 거부: `-----BEGIN PUBLIC KEY-----`, `-----BEGIN CERTIFICATE-----`, `ssh-ed25519 AAAA...`, 빈 입력, `-----BEGIN`만 있는 줄, 앞에 공백이나 다른 글자가 있는 줄(` -----BEGIN OPENSSH PRIVATE KEY-----`), `PuTTY-User-Key-File-3: ssh-ed25519`, `-----BEGIN OPENSSH PRIVATE KEY----- extra`

`parse_public_key_line`:

3. `ssh-ed25519 AAAA user@host`에서 종류와 코멘트
4. 코멘트에 공백이 있는 경우(`ssh-rsa AAAA my laptop key`의 코멘트는 `my laptop key`)
5. 코멘트 없음, 끝에 `\r\n`이 붙은 경우
6. `sk-ssh-ed25519@openssh.com AAAA yubikey`
7. 거부: 빈 줄, 필드 1개, 종류에 허용되지 않는 문자가 있는 줄

`discover_keys`:

8. 비밀키 2개, 각각의 `.pub`(하나는 있고 하나는 없음), `config`, `known_hosts`, `authorized_keys`가 있는 디렉터리에서 비밀키 2개만 파일명 순으로 반환하고 `public` 정보 일치
9. `.pub` 파일 자체는 목록에 없음
10. 하위 디렉터리 안의 키는 제외, 이름이 키처럼 보이는 디렉터리(`id_dir/`)도 제외
11. 첫 줄은 유효하고 둘째 줄부터 UTF-8이 아닌 바이트가 든 파일은 포함
12. 첫 줄이 256바이트를 넘는 파일(줄바꿈 없이 `-----BEGIN` 뒤에 `A` 1000개)은 제외
13. 빈 파일 제외
14. 없는 디렉터리는 빈 목록
15. `.pub`가 손상된 경우(빈 파일) 키는 포함되고 `public`은 `None`
16. (`#[cfg(unix)]`) 비밀키를 가리키는 심볼릭 링크는 포함, 권한 0000인 파일은 건너뜀
17. `label()` 3가지 형태

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- 위 테스트가 모두 존재하고 기존 테스트 150개 유지
