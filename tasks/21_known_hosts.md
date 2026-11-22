# 작업 21: known_hosts 항목 탐색과 삭제 로직

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다.

## 목표

선택한 호스트의 호스트 키를 `known_hosts`에서 지우는 기능의 UI 무관 부분을 만든다. 이 작업에서는 화면과 `Backend`를 건드리지 않는다(작업 22에서 연결).

- ssh가 호스트 키를 찾을 때 쓰는 이름 계산
- `known_hosts` 내용에서 그 이름의 줄을 찾아 지운 결과 생성(해시된 항목 포함)
- 파일을 읽어 `PendingWrite`로 만드는 얇은 계층

대상 파일은 항상 `SshPaths::ssh_dir()` 아래의 `known_hosts` 하나다. `known_hosts2`, `UserKnownHostsFile`로 지정한 다른 파일, `/etc/ssh/ssh_known_hosts`는 다루지 않는다. `ssh-keygen` 등 외부 명령은 실행하지 않는다.

## 대상 파일

- `src/known_hosts/mod.rs` (신규)
- `src/known_hosts/hash.rs` (신규)
- `src/lib.rs` (`pub mod known_hosts;` 추가)
- `src/config/plan.rs` (`PendingWrite` 공개 생성자 추가)
- `src/ssh/effective.rs` (`host_key_alias` 필드 추가)
- `src/app/tests.rs`, `src/ui/tests.rs`, `src/system.rs`: `EffectiveConfig` 구조체 리터럴이 컴파일되도록 새 필드를 채우는 수정만 허용(`host_key_alias: None`). 그 밖의 수정 금지.

`Cargo.toml` 수정 금지. 새 의존성 없이 구현한다.

## 1. `src/ssh/effective.rs`

`EffectiveConfig`에 필드 추가:

```rust
/// Name ssh uses instead of the host name when looking up host keys.
pub host_key_alias: Option<String>,
```

`parse_ssh_g`가 `hostkeyalias <값>` 줄을 이 필드에 넣는다. 기존 필드의 동작과 `find_mismatches`는 바꾸지 않는다. 테스트 추가: `hostkeyalias my-alias` 줄이 있을 때와 없을 때.

## 2. `src/config/plan.rs`

`PendingWrite`에 공개 생성자 추가(기존 비공개 `new`를 재사용):

```rust
/// Plans replacing an existing file whose loaded bytes are `original`.
pub fn replacement(path: PathBuf, original: &[u8], updated: Vec<u8>) -> Self
```

`original`은 `Some`으로 들어가고 `hunks`는 기존과 같이 `DIFF_CONTEXT`로 만든다. 테스트 1개 추가.

## 3. `src/known_hosts/hash.rs`

해시된 호스트 이름 대조에 필요한 최소 구현. 모두 순수 함수이고 `unsafe` 없음.

```rust
/// Returns the SHA-1 digest of `data`.
pub fn sha1(data: &[u8]) -> [u8; 20];
/// Returns HMAC-SHA1 of `data` under `key` (RFC 2104).
pub fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20];
/// Decodes standard base64 with `=` padding. Returns None for any invalid input.
pub fn base64_decode(input: &[u8]) -> Option<Vec<u8>>;
```

- `base64_decode`: 표준 알파벳(`A-Z a-z 0-9 + /`), 길이는 4의 배수, 패딩은 끝에만 0-2개. 공백과 다른 문자, 패딩 뒤의 문자, 잘못된 길이는 `None`. 빈 입력은 `Some(vec![])`.
- 이 모듈은 호스트 이름 대조 전용이라는 점을 모듈 문서 주석에 한 줄로 적는다.

필수 테스트(16진수 비교):

| 함수 | 입력 | 기대값 |
|---|---|---|
| `sha1` | `""` | `da39a3ee5e6b4b0d3255bfef95601890afd80709` |
| `sha1` | `"abc"` | `a9993e364706816aba3e25717850c26c9cd0d89d` |
| `sha1` | `"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"` | `84983e441c3bd26ebaae4aa1f95129e5e54670f1` |
| `sha1` | `'a'` 1,000,000개 | `34aa973cd4c4daa4f61eeb2bdbad27316534016f` |
| `hmac_sha1` | key `0x0b` 20개, data `"Hi There"` | `b617318655057264e28bc0b6fb378c8ef146be00` |
| `hmac_sha1` | key `"Jefe"`, data `"what do ya want for nothing?"` | `effcdf6ae5eb2fa2d27416d5f184df9c259a7c79` |
| `hmac_sha1` | key `0xaa` 20개, data `0xdd` 50개 | `125d7342b9ac11cd91a39af48aa17b4f63f175d3` |
| `hmac_sha1` | key `0xaa` 80개, data `"Test Using Larger Than Block-Size Key - Hash Key First"` | `aa4ae5e15272d00e95705637ce8a3b55ed402112` |

`base64_decode` 테스트: `"Zm9vYmFy"` → `foobar`, `"Zm9vYg=="` → `foob`, `"Zm9vYmE="` → `fooba`, 빈 입력, 잘못된 입력 5종 이상(`"Zm9v="`, `"Zm9vY"`, `"Zm9v Yg=="`, `"Zg=a"`, `"===="`, `"Z==="`).

## 4. `src/known_hosts/mod.rs`

```rust
pub mod hash;

/// File name of the per-user host key database inside the ssh directory.
pub const FILE_NAME: &str = "known_hosts";

/// Returns the known_hosts path inside the injected ssh directory.
pub fn file_path(paths: &SshPaths) -> PathBuf;

/// Returns the name ssh looks up for a connection.
pub fn lookup_name(host_key_alias: Option<&str>, host_name: &str, port: u16) -> String;

/// Returns the lookup name for an alias from its effective settings.
pub fn lookup_name_for(alias: &str, effective: &EffectiveConfig) -> String;

/// Returns whether a known_hosts line is a plain or hashed entry for exactly `name`.
pub fn line_matches(line: &[u8], name: &str) -> bool;

/// Result of removing the entries for one name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    /// File contents without the matching lines.
    pub updated: Vec<u8>,
    /// 1-based numbers of the removed lines, ascending.
    pub removed_lines: Vec<usize>,
}

/// Removes every line that `line_matches` for `name`, preserving all other bytes.
pub fn remove_entries(contents: &[u8], name: &str) -> Removal;

/// A planned removal awaiting confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetPlan {
    /// Lookup name whose entries will be removed.
    pub name: String,
    /// Number of lines that will be removed.
    pub removed: usize,
    /// The file replacement to commit.
    pub pending: PendingWrite,
}

/// Errors from reading the known_hosts file.
#[derive(Debug, thiserror::Error)]
pub enum KnownHostsError { /* Read { path, source } */ }

/// Reads known_hosts and plans removing the entries for `name`.
/// Returns Ok(None) when the file does not exist or nothing matches.
pub fn plan_forget(paths: &SshPaths, name: &str) -> Result<Option<ForgetPlan>, KnownHostsError>;
```

### 이름 계산 (`lookup_name`, `lookup_name_for`)

OpenSSH의 동작을 따른다.

- `host_key_alias`가 `Some`이고 비어 있지 않으면 그 값을 ASCII 소문자로 바꿔 그대로 반환(포트를 붙이지 않음).
- 그 외에는 `host_name`을 ASCII 소문자로 바꾼 뒤, `port == 22`이면 그대로, 아니면 `[host]:port` 형식. IPv6 주소도 같은 규칙(`::1`, `[::1]:2222`).
- `lookup_name_for`: `effective.host_key_alias`, `effective.host_name`(없으면 `alias`), `effective.port`(`u16`로 파싱, 없거나 파싱 실패 또는 0이면 22)로 `lookup_name` 호출.

### 줄 대조 (`line_matches`)

입력은 한 줄의 바이트(끝의 `\n`, `\r\n`이 있어도 되고 없어도 됨). UTF-8이 아닌 바이트가 있어도 패닉 없이 동작.

1. 줄 끝 문자(`\r`, `\n`)를 떼고 앞의 공백과 탭을 건너뜀. 비었거나 `#`으로 시작하면 `false`.
2. `@`로 시작하면(`@cert-authority`, `@revoked` 등 마커 줄) `false`. 마커 줄은 지우지 않는다.
3. 첫 필드(공백이나 탭 전까지)가 호스트 필드. 그 뒤에 공백을 건너뛰고 내용이 하나도 없으면 `false`.
4. 호스트 필드가 `|`로 시작하면 해시 항목: 정확히 `|1|<base64 salt>|<base64 hash>` 형식이어야 하고, 두 값을 `base64_decode`한 뒤 hash가 20바이트이고 `hmac_sha1(salt, name)`과 같으면 `true`. 형식이 다르면 `false`.
5. 그 외에는 `,`로 나눈 패턴 가운데 하나라도 ASCII 대소문자 무시로 `name`과 완전히 같으면 `true`. `*`나 `?`가 들어 있거나 `!`로 시작하는 패턴은 일치로 보지 않는다(와일드카드로만 걸리는 줄은 지우지 않음. `ssh-keygen -R`보다 보수적인 동작이며 문서 주석에 적는다).

`name`이 빈 문자열이면 항상 `false`.

### 삭제 (`remove_entries`)

- 내용을 `\n` 기준으로 줄 단위로 나누되 줄 끝 문자를 줄에 포함해 다룬다. 일치한 줄은 줄 끝 문자까지 통째로 제거, 나머지 바이트는 그대로 보존(`\r\n`, 마지막 줄의 줄 끝 없음, UTF-8이 아닌 바이트 포함).
- 일치가 없으면 `updated == contents`, `removed_lines`는 빈 벡터.

### 계획 (`plan_forget`)

- `file_path(paths)`를 읽는다. 파일이 없으면(`io::ErrorKind::NotFound`) `Ok(None)`. 다른 읽기 오류는 `KnownHostsError::Read`(메시지 예: `cannot read <path>: <원인>`).
- `remove_entries` 결과가 비면 `Ok(None)`.
- 아니면 `PendingWrite::replacement(path, &contents, updated)`로 `ForgetPlan`을 만든다. 이 함수는 파일을 쓰지 않는다.

## 필수 테스트 (`src/known_hosts/mod.rs`)

표본 키(공개 키뿐이며 어떤 실제 호스트의 것도 아님):

```
ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEk9zFM25++kJaRZ0WiPq2Rf/WXTnx0PsWWfN4crdP6X
```

아래 9줄짜리 표본 파일(`KEY`는 위 문자열로 치환, 각 줄은 `\n`으로 끝남)을 상수로 둔다. 8번과 9번 줄은 `ssh-keygen -H`가 만든 실제 해시 항목이며 각각 `example.com`과 `[10.0.0.5]:2222`의 해시다.

```
1  # comment
2  example.com,192.0.2.1 KEY
3  *.example.com KEY
4  @cert-authority *.example.com KEY
5  EXAMPLE.com KEY
6  other.example.com KEY
7  !example.com,* KEY
8  |1|jN+dgNbNb3ppckN470BGDfcIluM=|BBVTRb++39vYeyXY3Bx9uWLpByY= KEY
9  |1|jBdJaGrc1+QnnZl01Cq2lS9csU0=|41IYMBZM4Dz9DCfcmHdB3apXY6k= KEY
```

- `remove_entries(표본, "example.com")` → `removed_lines == [2, 5, 8]`, `updated`는 1, 3, 4, 6, 7, 9번 줄을 이은 바이트와 동일(`ssh-keygen -R example.com`의 실제 결과와 같음)
- `remove_entries(표본, "[10.0.0.5]:2222")` → `removed_lines == [9]`
- `remove_entries(표본, "a.example.com")` → 변화 없음(와일드카드 줄 유지)
- `remove_entries(표본, "192.0.2.1")` → `[2]`
- `remove_entries(표본, "10.0.0.5")` → 변화 없음(포트가 다른 이름)
- `lookup_name`: `(None, "Example.COM", 22)` → `example.com`, `(None, "10.0.0.5", 2222)` → `[10.0.0.5]:2222`, `(Some("My-Alias"), "x", 2222)` → `my-alias`, `(Some(""), "h", 22)` → `h`, `(None, "::1", 2222)` → `[::1]:2222`
- `lookup_name_for`: host_name 없음, port 없음, port `"abc"`, port `"0"`, host_key_alias 있음 각각
- `line_matches`: 앞에 공백이 있는 줄, 탭으로 구분한 줄, 호스트 필드만 있고 키가 없는 줄(`false`), `|1|` 형식 오류 3종(구분자 부족, base64 오류, hash 길이 불일치), `|2|...`, 빈 `name`, UTF-8이 아닌 바이트가 든 줄
- 줄 끝 처리: `\r\n` 파일에서 중간 줄 삭제 후 나머지 바이트 보존, 줄 끝 없는 마지막 줄 삭제, 줄 끝 없는 마지막 줄을 남기고 중간 줄 삭제, 모든 줄이 지워져 빈 결과
- `plan_forget`(`tempfile` 임시 디렉터리와 `SshPaths::resolve` 사용): 파일 없음 → `Ok(None)`, 일치 없음 → `Ok(None)`, 일치 있음 → `removed`, `name`, `pending.path == file_path(paths)`, `pending.original`, `pending.updated` 확인. 이어서 `pending.commit()` 후 파일 내용과 백업 파일(`.known_hosts.sshtui.bak`) 내용 확인. 계획 뒤 파일이 바뀌면 `commit()`이 `WriteError::Stale`
- `plan_forget`: 경로가 디렉터리일 때 `KnownHostsError::Read`

## 제약

- 실제 `~/.ssh` 접근 금지. 경로는 `SshPaths`로만 얻는다.
- 호스트 키 내용을 로그나 오류 메시지에 넣지 않는다.
- 라이브러리 코드에서 `unwrap`, `expect`, `panic!`, 범위를 벗어날 수 있는 인덱싱으로 인한 패닉 금지(고정 크기 블록 처리의 인덱싱은 허용).
- 공개 항목에 영어 문서 주석.

## 완료 조건

- `AGENTS.md`의 공통 완료 조건 3개 통과
- 기존 테스트의 단언 변경 없음(허용된 구조체 리터럴 필드 추가 제외)
- `git status`에 대상 파일만 변경으로 표시
