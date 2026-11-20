# 작업 5: config 쓰기, diff, 변경 계획

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `src/config/`의 기존 모듈(`model`, `reader`, `edit`)과 `src/paths.rs`, `src/validate.rs`를 읽고 그 타입을 사용한다.

## 목표

1. `diff`: 저장 전 확인 화면에 보여줄 줄 단위 diff(순수 함수)
2. `writer`: 외부 변경 감지, 백업, 임시 파일 교체로 파일을 안전하게 쓰는 계층
3. `plan`: reader, edit, diff, writer를 묶어 "추가, 편집, 삭제" 요청을 미리보기 가능한 쓰기 계획으로 만드는 계층

## 대상 파일

- `src/config/diff.rs`
- `src/config/writer.rs`
- `src/config/plan.rs` (신규)
- `src/config/mod.rs` (`pub mod plan;`과 재노출 추가)

## src/config/diff.rs

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffTag { Context, Removed, Added }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub tag: DiffTag,
    /// 1-based line number in the old file (None for Added).
    pub old_line: Option<usize>,
    /// 1-based line number in the new file (None for Removed).
    pub new_line: Option<usize>,
    /// Line content without its terminator (lossy UTF-8).
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk { pub lines: Vec<DiffLine> }

/// Line diff of two files with `context` unchanged lines around each change.
/// Lines are compared as bytes including their terminators, so a terminator-only change is a change.
pub fn diff_lines(old: &[u8], new: &[u8], context: usize) -> Vec<DiffHunk>;
```

- 줄 분리는 `ParsedConfig::parse`와 같은 규칙(`\n` 기준, 종결자 포함 비교)을 쓴다. `ParsedConfig`를 재사용해도 된다.
- 알고리즘은 `similar` crate의 `capture_diff_slices(Algorithm::Myers, ...)`와 `group_diff_ops(ops, context)`를 사용한다. crate 소스는 `~/.cargo/registry/src/*/similar-3.2.1/`에서 읽을 수 있다.
- 한 hunk 안에서 교체(`Replace`)는 `Removed` 줄들 다음에 `Added` 줄들 순서로 낸다.
- 내용이 같으면 빈 벡터.

테스트: 줄 추가, 줄 삭제, 줄 수정(Removed 다음 Added), 줄 번호(old와 new), context 크기에 따른 hunk 분리와 병합, 변경 없음, 빈 파일에서 생성, 전체 삭제, CRLF 파일(텍스트에 `\r` 없음), 종결자만 바뀐 줄(`a\n`과 `a\r\n`)이 변경으로 나옴, 마지막 줄 종결자 유무 차이.

## src/config/writer.rs

```rust
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The file no longer matches the bytes that were loaded (or appeared/disappeared).
    #[error("{path} was changed by another program; reload and try again")]
    Stale { path: PathBuf },
    #[error("cannot {action} {path}: {source}")]
    Io { action: &'static str, path: PathBuf, #[source] source: std::io::Error },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOutcome {
    /// The real file that was written (symlinks resolved).
    pub target: PathBuf,
    /// Backup of the previous contents; None when the file was newly created.
    pub backup: Option<PathBuf>,
}

/// Name of the backup file for `file_name`: `.<file_name>.sshtui.bak`.
pub fn backup_file_name(file_name: &OsStr) -> OsString;

/// Replaces the contents of an existing file.
pub fn replace_file(path: &Path, original: &[u8], updated: &[u8]) -> Result<WriteOutcome, WriteError>;

/// Creates a file that must not exist yet, creating its parent directory if needed.
pub fn create_file(path: &Path, contents: &[u8]) -> Result<WriteOutcome, WriteError>;

/// True when the existing file can be opened for writing. Never modifies the file.
pub fn is_writable(path: &Path) -> bool;
```

`replace_file` 절차(순서 고정):

1. `target = std::fs::canonicalize(path)`. 심볼릭 링크는 링크를 유지하고 실제 대상 파일을 수정하기 위함이다. 없으면(`NotFound`) `Stale`.
2. `target`을 다시 읽어 `original`과 바이트가 다르면 `Stale`. 이 경우 아무 파일도 만들거나 바꾸지 않는다.
3. 백업: `target`과 같은 디렉터리에 `backup_file_name(target의 파일명)`으로 `original` 내용을 저장한다. 기존 백업은 덮어쓴다. 백업 파일의 권한은 `target`의 권한과 같게 한다. 백업 이름이 `.`으로 시작하는 이유는 `Include conf.d/*` 같은 glob에 백업이 걸리지 않게 하기 위함이므로 이름 규칙을 바꾸지 않는다.
4. `target`과 같은 디렉터리에 임시 파일(`tempfile::Builder`, 접두사 `.sshtui-`)을 만들어 `updated`를 쓰고, 권한을 `target`의 기존 권한(`fs::metadata(target).permissions()`)과 같게 설정하고, `sync_all` 후 `persist(target)`로 교체한다.
5. 백업도 임시 파일에 쓴 뒤 `persist`하는 방식으로 만든다(부분 기록된 백업 방지).
6. 어느 단계에서 실패하든 임시 파일이 남지 않아야 한다. 4단계가 실패해도 `target`의 내용은 `original` 그대로여야 한다.

`create_file` 절차:

1. 부모 디렉터리가 없으면 만든다(`create_dir_all`). Unix에서는 새로 만든 디렉터리의 권한을 0700으로 한다(`DirBuilderExt::mode`).
2. 같은 디렉터리의 임시 파일에 `contents`를 쓰고 Unix에서는 권한 0600으로 설정한 뒤 `persist_noclobber(path)`로 옮긴다. 대상이 이미 있으면 `Stale`.
3. 반환값의 `target`은 `path`, `backup`은 `None`.

`is_writable`: 파일을 `OpenOptions::new().write(true)`로 열어 성공 여부만 본다(truncate와 create 금지). 없는 파일은 `false`.

테스트(모두 `tempfile::tempdir()` 안에서):

1. 교체 성공: 내용이 `updated`와 일치, `WriteOutcome::target`과 `backup` 경로 확인
2. 백업: 이름이 `.config.sshtui.bak`, 내용이 `original`과 일치, 두 번째 교체 시 백업이 직전 내용으로 갱신
3. 외부 변경 감지: 불러온 뒤 파일을 바꾸면 `Stale`, 파일 내용은 외부에서 바꾼 그대로, 백업 미생성
4. 파일이 삭제된 경우 `Stale`
5. 교체 후 디렉터리에 임시 파일이 없음(디렉터리 목록이 대상 파일과 백업 2개뿐)
6. `Stale` 이후에도 임시 파일 없음
7. CRLF와 UTF-8이 아닌 바이트가 그대로 기록됨
8. (`#[cfg(unix)]`) 권한 유지: 0640 파일은 교체 후에도 0640, 백업도 0640
9. (`#[cfg(unix)]`) 심볼릭 링크: `link -> real/config`에서 `replace_file(link, ...)` 후 `link`는 여전히 심볼릭 링크(`symlink_metadata`), `real/config` 내용 갱신, 백업은 `real/` 디렉터리에 생성, `link`가 있는 디렉터리에는 새 파일 없음
10. (`#[cfg(unix)]`) 링크의 링크(2단계)도 최종 대상 수정
11. `create_file`: 없는 디렉터리와 파일 생성, 내용 일치, (`#[cfg(unix)]`) 디렉터리 0700과 파일 0600
12. `create_file`: 이미 있으면 `Stale`이고 기존 내용 유지, 임시 파일 없음
13. `is_writable`: 일반 파일 true, 없는 파일 false, (`#[cfg(unix)]`) 0400 파일 false. 호출 전후 파일 내용 동일
14. `backup_file_name("config")`는 `.config.sshtui.bak`, `backup_file_name("web.conf")`는 `.web.conf.sshtui.bak`

## src/config/plan.rs

```rust
use crate::config::{ConfigSet, HostEntry, NewHost, OptionChanges, EditError};
use crate::config::diff::DiffHunk;
use crate::config::writer::{WriteError, WriteOutcome};
use crate::validate::ValidationError;

/// Number of context lines in previews.
pub const DIFF_CONTEXT: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error(transparent)]
    Edit(#[from] EditError),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// The entry does not match the loaded configuration any more.
    #[error("host {0:?} is no longer present in the loaded configuration; reload and try again")]
    HostNotFound(String),
}

/// A prepared change to one file, not yet written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingWrite {
    /// File to write (as recorded in `LoadedFile::path`, or the root config path when creating).
    pub path: PathBuf,
    /// Bytes loaded from the file; None when the file does not exist yet.
    pub original: Option<Vec<u8>>,
    pub updated: Vec<u8>,
    /// Preview of the change with `DIFF_CONTEXT` lines of context.
    pub hunks: Vec<DiffHunk>,
}
impl PendingWrite {
    /// True when nothing would change.
    pub fn is_noop(&self) -> bool;
    /// Writes the change: `create_file` when `original` is None, `replace_file` otherwise.
    /// A no-op returns Ok without touching the file system (`target = path`, `backup = None`).
    pub fn commit(&self) -> Result<WriteOutcome, WriteError>;
}

/// Plans adding a new host block to the root config (creating it when missing).
pub fn plan_add(paths: &SshPaths, set: &ConfigSet, host: &NewHost) -> Result<PendingWrite, PlanError>;

/// Plans renaming `entry` and/or changing the options of its block, in the file that defines it.
pub fn plan_edit(
    set: &ConfigSet,
    entry: &HostEntry,
    new_name: Option<&str>,
    changes: &OptionChanges,
) -> Result<PendingWrite, PlanError>;

/// Plans removing `entry` (the whole block when it is the only pattern).
pub fn plan_delete(set: &ConfigSet, entry: &HostEntry) -> Result<PendingWrite, PlanError>;
```

규칙:

- `plan_add`
  - `validate_nickname`, 이어서 `set.hosts`의 모든 이름(Include 대상 파일 포함)에 대해 `check_nickname_unique`.
  - 루트 파일은 `set.files`에서 `path == paths.config_file()`인 항목이다. 있으면 그 `parsed`에 `add_host`, `original = Some(bytes)`. 없으면 빈 `ParsedConfig`에 `add_host`, `original = None`, `path = paths.config_file()`.
- `plan_edit`
  - `entry`가 가리키는 위치 확인: `set.files[entry.file_index]`가 있고, 그 파일의 `blocks()[entry.block_index]` 헤더의 `args[entry.pattern_index].value == entry.name`이어야 한다. 아니면 `HostNotFound(entry.name)`.
  - `new_name`이 `Some`이고 `entry.name`과 다르면 `validate_nickname` 후 `set.hosts` 중 `entry` 자신(같은 `file_index`, `block_index`, `pattern_index`)을 제외한 이름에 대해 `check_nickname_unique`.
  - `apply_host_edit` 결과로 `PendingWrite` 생성. `path`는 `set.files[entry.file_index].path`.
- `plan_delete`: 위치 확인 후 `delete_pattern`.
- 세 함수 모두 파일 시스템에 접근하지 않는다. 파일 시스템 접근은 `commit`뿐이다.

테스트(`tempfile::tempdir()`, 경로 주입은 `SshPaths::resolve(Some(<tmp>/ssh), Some(<tmp>/home), <tmp>)`):

1. config가 없는 상태에서 `plan_add`: `original`이 None, hunk가 전부 `Added`, `commit` 후 파일 생성과 내용 일치, 다시 `load`하면 호스트 1개
2. 기존 config에 `plan_add` 후 `commit`, 백업 생성, 다시 `load`한 목록에 새 호스트 포함
3. 중복 닉네임 거부: 루트의 이름, Include 대상 파일의 이름 모두 `PlanError::Validation(DuplicateNickname)`
4. Include 대상 파일의 호스트를 `plan_edit` 후 `commit`: 그 파일만 바뀌고 루트 파일의 바이트는 동일, `PendingWrite::path`가 포함 파일 경로
5. 이름 변경: 다른 호스트와 같은 이름은 거부, 자기 이름 그대로는 허용(이 경우 옵션 변경이 없으면 `is_noop()`)
6. 여러 패턴 블록(`Host db cache`)에서 `cache` 항목의 `plan_edit` 이름 변경과 `plan_delete`가 그 패턴만 처리
7. `plan_delete` 후 `commit`, 다시 `load`한 목록에서 제거
8. 외부 변경: `load` 후 파일을 직접 수정하고 `commit`하면 `WriteError::Stale`, 파일은 외부 수정 내용 그대로
9. `HostNotFound`: 범위 밖 `file_index`, 이름이 맞지 않는 `entry`
10. no-op `commit`은 백업을 만들지 않고 파일 수정 시각 관련 부작용 없이 `Ok`(디렉터리 목록 불변으로 확인)
11. hunk 내용: 옵션 하나를 바꾼 `plan_edit`의 hunk에 `Removed` 1줄과 `Added` 1줄이 있고 텍스트가 기대값과 일치
12. (`#[cfg(unix)]`) 루트 config가 심볼릭 링크일 때 `plan_add`와 `commit` 후 링크 유지, 대상 파일 갱신

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- 위 테스트가 모두 존재하고 기존 테스트 113개 유지
- 모든 테스트가 임시 디렉터리만 사용
