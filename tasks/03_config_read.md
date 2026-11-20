# 작업 3: config 읽기 (lexer, model, reader)

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `src/paths.rs`의 `SshPaths`를 사용한다.

## 목표

ssh_config 파일을 원문 바이트를 보존하는 구조로 파싱하고, `Include`를 확장해 `Host` 항목 목록을 수집한다. 이 구조는 다음 작업(편집)에서 바이트 범위 단위 수정에 쓰이므로 모든 위치 정보는 파일 전체 기준 바이트 오프셋이다.

해석 규칙(어떤 값이 실제로 적용되는지)은 구현하지 않는다. 문법만 다룬다.

## 대상 파일

- `src/config/lexer.rs`
- `src/config/model.rs`
- `src/config/reader.rs`
- `src/config/mod.rs` (재노출 `pub use`만 추가 가능)

## 문법 (OpenSSH `readconf.c`와 `misc.c`의 `argv_split` 동작 기준)

줄 분리:

- 파일은 `\n`으로 줄을 나눈다. `\n` 바로 앞의 `\r` 하나는 줄바꿈의 일부(CRLF)다. 그 외 위치의 `\r`은 줄 내용이다.
- 마지막 줄은 줄바꿈 없이 끝날 수 있다. 빈 파일은 줄이 0개다. 파일이 `\n`으로 끝나면 그 뒤에 빈 줄을 추가로 만들지 않는다.

한 줄의 해석(줄바꿈을 제외한 내용에 대해):

1. 끝의 공백, 탭, `\r`, `\x0c`(form feed)는 무시한다. 앞의 공백, 탭, `\r`은 건너뛴다.
2. 남은 내용이 없으면 `Blank`, `#`로 시작하면 `Comment`.
3. 키워드: 공백, 탭, `=`, `"` 중 하나가 나오기 전까지의 연속 바이트. 대소문자를 구분하지 않고 비교한다.
4. 구분자: 키워드 뒤의 (공백 또는 탭)* 다음에 `=`가 있으면 그 `=` 하나와 뒤따르는 (공백 또는 탭)*까지. `=`는 하나만 구분자다. `Port=22`, `Port = 22`, `Port =22`, `Port= 22`는 모두 키워드 `Port`, 인자 `22`. `Port == 22`는 인자가 `=`와 `22` 두 개.
5. 인자 분리(구분자 뒤 나머지에 대해 반복):
   - 공백과 탭을 건너뛴다.
   - 토큰 시작 위치의 문자가 `#`이면 그 위치부터 줄 끝까지가 후행 주석이다. 토큰 중간의 `#`(`a#b`)는 글자다.
   - 토큰 내부 처리: 따옴표 밖에서 `"` 또는 `'`를 만나면 그 문자로 따옴표가 열리고, 같은 문자를 만나면 닫힌다. 따옴표 문자 자체는 값에 포함되지 않는다. 따옴표는 토큰 중간에서도 열릴 수 있다(`a"b c"d`의 값은 `ab cd`).
   - 백슬래시: 다음 문자가 `'`, `"`, `\`이면(따옴표 안팎 모두), 또는 따옴표 밖에서 다음 문자가 공백이면, 백슬래시를 버리고 다음 문자를 값에 넣는다. 그 외의 백슬래시는 글자 그대로 값에 넣는다(`C:\Users\me`의 값은 `C:\Users\me`).
   - 따옴표 밖에서 공백 또는 탭을 만나면 토큰이 끝난다.
   - 줄 끝까지 따옴표가 닫히지 않으면 그 줄 전체가 `Malformed`.
6. 인자가 0개인 지시어도 `Directive`로 둔다(`args`가 빈 벡터).

블록:

- 키워드가 `Host` 또는 `Match`인 `Directive` 줄이 블록의 헤더다. 블록은 헤더 줄부터 다음 헤더 줄 직전(또는 파일 끝)까지다.
- 첫 헤더 이전의 줄은 어떤 블록에도 속하지 않는다.

## src/config/model.rs

```rust
use std::ops::Range;

/// Line terminator of a physical line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol { None, Lf, CrLf }
impl Eol { pub fn as_bytes(self) -> &'static [u8]; }

/// One argument of a directive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    /// Unquoted, unescaped value (lossy UTF-8).
    pub value: String,
    /// Byte range in the file covering the raw token, including quotes and escapes.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    /// Keyword as written (lossy UTF-8).
    pub keyword: String,
    pub keyword_span: Range<usize>,
    pub args: Vec<Arg>,
    /// Trailing comment from its `#` to the end of the line content (trailing whitespace included).
    pub comment_span: Option<Range<usize>>,
}
impl Directive {
    /// ASCII case-insensitive keyword comparison.
    pub fn is(&self, keyword: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind { Blank, Comment, Directive(Directive), Malformed }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Byte range of the line content, excluding the terminator.
    pub span: Range<usize>,
    pub eol: Eol,
    pub kind: LineKind,
}
impl Line {
    /// Byte range including the terminator.
    pub fn full_span(&self) -> Range<usize>;
    pub fn directive(&self) -> Option<&Directive>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind { Host, Match }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// Index of the header line in `ParsedConfig::lines()`.
    pub header: usize,
    /// Exclusive end line index.
    pub end: usize,
}

/// A parsed config file that keeps the original bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedConfig { /* private: bytes: Vec<u8>, lines: Vec<Line>, blocks: Vec<Block> */ }
impl ParsedConfig {
    /// Parses any byte sequence. Never fails; unparseable lines become `LineKind::Malformed`.
    pub fn parse(bytes: Vec<u8>) -> Self;
    pub fn bytes(&self) -> &[u8];
    pub fn lines(&self) -> &[Line];
    pub fn blocks(&self) -> &[Block];
    /// Content bytes of line `index` without the terminator.
    pub fn line_bytes(&self, index: usize) -> &[u8];
    /// Index of the block that contains line `index`, if any.
    pub fn block_of_line(&self, index: usize) -> Option<usize>;
}
```

불변식: 모든 줄의 `full_span()`은 0에서 시작해 빈틈과 겹침 없이 이어지고 마지막 줄의 끝은 `bytes().len()`이다.

## src/config/lexer.rs

```rust
/// Classifies one line. `content` is the line without its terminator and `offset` is the
/// byte offset of `content` within the file; all returned spans are absolute.
pub fn lex_line(content: &[u8], offset: usize) -> LineKind;
```

입출력 없는 순수 함수다. `ParsedConfig::parse`가 이 함수를 사용한다.

## src/config/reader.rs

```rust
/// A concrete host alias found on a `Host` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    /// The alias (one pattern of the Host line).
    pub name: String,
    /// File that defines it.
    pub source: PathBuf,
    /// Index into `ConfigSet::files`.
    pub file_index: usize,
    /// Index into that file's `ParsedConfig::blocks()`.
    pub block_index: usize,
    /// Index into the header directive's `args`.
    pub pattern_index: usize,
    /// 1-based line number of the Host line.
    pub line_number: usize,
    /// Every other pattern on the same Host line, in order, including wildcard and negated ones.
    pub siblings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFile { pub path: PathBuf, pub parsed: ParsedConfig }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadWarning {
    /// An included file exists but could not be read.
    Unreadable { path: PathBuf, error: String },
    /// The Include argument uses `${VAR}` or `%` tokens, which are not expanded.
    UnsupportedInclude { file: PathBuf, line_number: usize, pattern: String },
    /// The Include argument is not a valid glob pattern.
    InvalidGlob { file: PathBuf, line_number: usize, pattern: String },
    /// Include nesting deeper than 16 levels.
    DepthExceeded { path: PathBuf },
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("cannot read {path}: {source}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigSet {
    /// Root config first, then included files in discovery order. Each file appears once.
    pub files: Vec<LoadedFile>,
    /// Listable hosts in the order ssh would encounter them.
    pub hosts: Vec<HostEntry>,
    pub warnings: Vec<ReadWarning>,
}

/// True when `pattern` is a plain alias: no `*`, no `?`, and not starting with `!`.
pub fn is_listable_pattern(pattern: &str) -> bool;

/// Pure: resolves one Include argument to the glob pattern path to expand.
/// `~` and `~/...` use `paths.expand_tilde`; absolute paths are kept; anything else is
/// joined to `paths.ssh_dir()` (NOT to the including file's directory).
/// Returns `None` when the argument contains `${` or `%`.
pub fn resolve_include_pattern(paths: &SshPaths, arg: &str) -> Option<PathBuf>;

/// Loads `paths.config_file()` and everything it includes.
pub fn load(paths: &SshPaths) -> Result<ConfigSet, ReadError>;
```

`load` 규칙:

- 루트 파일이 없으면(`NotFound`) 빈 `ConfigSet`을 `Ok`로 반환한다. 다른 읽기 오류는 `ReadError::Io`.
- 루트 파일의 줄을 순서대로 처리한다.
  - `Host` 헤더: 인자(패턴)마다 `is_listable_pattern`이면 `HostEntry`를 만든다. `Match` 블록은 항목을 만들지 않는다.
  - `Include` 지시어(블록 안팎 모두): 인자마다 `resolve_include_pattern`, 이어서 `glob::glob_with`로 확장한다. 옵션은 `case_sensitive: true`, `require_literal_separator: true`, `require_literal_leading_dot: true`. 결과는 사전순으로 처리하고 일반 파일만 대상으로 한다. 일치하는 파일이 없으면 아무 일도 하지 않는다(경고 없음). 포함된 파일은 그 `Include` 위치에서 재귀적으로 같은 방식으로 처리하므로, 포함된 파일의 호스트는 `hosts`에서 그 위치에 놓인다.
- 같은 파일(`std::fs::canonicalize` 결과 기준)은 한 번만 읽고 한 번만 처리한다. 순환 Include에서 무한 루프가 생기면 안 된다.
- 중첩 깊이는 루트를 0으로 하여 16을 넘으면 `DepthExceeded` 경고를 남기고 그 파일을 건너뛴다.
- 포함된 파일을 읽지 못하면 `Unreadable` 경고를 남기고 계속한다.
- `LoadedFile::path`와 `HostEntry::source`에는 canonicalize하지 않은 경로(루트는 `paths.config_file()`, 포함 파일은 glob이 돌려준 경로)를 넣는다.
- 같은 이름의 호스트가 여러 블록에 있으면 각각 별도 항목으로 둔다.

## 테스트

모든 파일 시스템 테스트는 `tempfile::tempdir()` 안에서만 수행하고 `SshPaths::resolve(Some(<tmp>/ssh), Some(<tmp>/home), <tmp>)`로 경로를 주입한다. 실제 홈 디렉터리와 `SshPaths::from_env()`는 사용하지 않는다.

lexer와 model:

1. `Blank`, `Comment`(들여쓴 `#` 포함) 판별
2. 키워드와 인자의 값과 span: `"  HostName example.com"`에서 span으로 원문을 잘라내면 `HostName`, `example.com`
3. 구분자 4종(`Port=22`, `Port = 22`, `Port =22`, `Port= 22`)과 `Port == 22`
4. 큰따옴표와 작은따옴표: `IdentityFile "~/my keys/id"`의 값은 `~/my keys/id`, span은 따옴표 포함. `a"b c"d`의 값은 `ab cd`
5. 백슬래시: `"a\"b"`의 값은 `a"b`, `C:\Users\me\key`는 그대로, `\\\\server\\share`의 값은 `\\server\share`, 따옴표 밖 `my\ key`의 값은 `my key`
6. 후행 주석: `Port 22 # note`의 인자는 1개, `comment_span`으로 자른 원문은 `# note`. `Host a#b`의 인자 값은 `a#b`이고 주석 없음
7. 닫히지 않은 따옴표는 `Malformed`
8. 인자 없는 지시어(`Host`만 있는 줄)는 `args`가 빈 `Directive`
9. `Directive::is`의 대소문자 무시(`hostname`, `HOSTNAME`)
10. CRLF 파일: 줄마다 `Eol::CrLf`, span에 `\r` 미포함, 인자 값에 `\r` 미포함
11. 마지막 줄에 줄바꿈이 없는 파일, 빈 파일, LF와 CRLF가 섞인 파일
12. 불변식: 여러 입력(위 사례 포함, UTF-8이 아닌 바이트가 든 입력 포함)에 대해 `full_span()`을 이어 붙인 바이트가 원본과 같음
13. 블록 경계: 전역 옵션, `Host a`, `Host b c`, `Match host x`, `Host *`가 있는 입력에서 블록 수, `kind`, `header`, `end`, `block_of_line`

reader:

14. Host 추출: 이름, `line_number`, `block_index`, `pattern_index`, `source`
15. 와일드카드 제외: `Host *`, `Host *.example.com`, `Host web-?`, `Host !secret`은 목록에 없음. `Match` 블록의 내용도 없음
16. 여러 패턴: `Host web db *.internal`에서 `web`과 `db` 두 항목, 각 `pattern_index`와 `siblings`(와일드카드 포함) 확인
17. 대소문자와 `=` 표기: `host=web`, `HOST web2`도 추출
18. Include 확장: 상대 경로(`Include conf.d/*.conf`)가 ssh 디렉터리 기준으로 풀리고, 포함 파일이 다른 디렉터리에 있어도 그 파일 안의 상대 Include 역시 ssh 디렉터리 기준임
19. Include glob 사전순 처리와 호스트 순서(Include 위치에 삽입)
20. `Include ~/extra/config`가 주입된 home 기준으로 풀림. 절대 경로 Include
21. 한 줄에 여러 Include 인자
22. 일치 없는 glob과 없는 파일은 경고 없이 무시
23. 순환 Include(a가 b를, b가 a를 포함)에서 종료하고 각 파일은 한 번만 수록
24. `Include ${HOME}/x`와 `Include %d/x`는 `UnsupportedInclude` 경고
25. 루트 파일이 없으면 빈 `ConfigSet`
26. glob의 `*`는 `.`으로 시작하는 파일과 디렉터리 구분자에 일치하지 않음
27. `resolve_include_pattern`의 4가지 경우
28. (`#[cfg(unix)]`) 심볼릭 링크로 같은 파일을 두 경로로 Include해도 한 번만 처리

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- 위 테스트 목록이 모두 존재하고 기존 테스트 27개 유지
