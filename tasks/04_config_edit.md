# 작업 4: config 편집 순수 함수

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. `src/config/model.rs`와 `src/config/lexer.rs`를 읽고 그 타입을 사용한다.

## 목표

`ParsedConfig`를 입력으로 받아 수정된 파일 전체 바이트(`Vec<u8>`)를 돌려주는 순수 함수를 구현한다. 파일 시스템에는 접근하지 않는다.

핵심 요구는 최소 변경이다. 바뀌는 옵션의 줄만 수정, 추가, 삭제하고 그 외 바이트(주석, 빈 줄, 들여쓰기, 줄바꿈 형식, 키워드 대소문자, 구분자 표기, 후행 주석, 모르는 옵션, 다른 블록, UTF-8이 아닌 바이트)는 그대로 보존한다. 구현은 원본 바이트에 대한 (바이트 범위, 대체 바이트) 목록을 만들어 한 번에 적용하는 방식으로 한다. 파일을 다시 직렬화하는 방식은 금지한다.

## 대상 파일

- `src/config/edit.rs`
- `src/config/mod.rs` (재노출만)
- `src/config/reader.rs` (아래 "추가 테스트 1건"만. 구현 코드는 수정 금지)

## API

```rust
use crate::validate::ValidationError;

/// The four options the form can edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionKey { HostName, User, Port, IdentityFile }
impl OptionKey {
    pub const ALL: [OptionKey; 4]; // HostName, User, Port, IdentityFile (canonical order)
    /// Canonical spelling written for new lines: "HostName", "User", "Port", "IdentityFile".
    pub fn keyword(self) -> &'static str;
}

/// How one option appears inside a single block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldState {
    /// No directive with this keyword in the block.
    Absent,
    /// Exactly one directive with exactly one argument.
    Value(String),
    /// Anything else (several directives, or a directive with zero or several arguments).
    /// The form shows it read-only. `lines` holds the raw content of each such line (lossy UTF-8).
    Unrepresentable { lines: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockFields {
    pub host_name: FieldState,
    pub user: FieldState,
    pub port: FieldState,
    pub identity_file: FieldState,
}
impl BlockFields { pub fn get(&self, key: OptionKey) -> &FieldState; }

/// Requested change for one option.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Change { #[default] Keep, Set(String), Remove }
impl Change {
    /// Derives the change from the current state and the form text.
    /// Unrepresentable -> Keep. Text equal to the current value -> Keep.
    /// Empty text: Absent -> Keep, Value -> Remove. Otherwise Set(text).
    pub fn from_form(current: &FieldState, text: &str) -> Change;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OptionChanges {
    pub host_name: Change,
    pub user: Change,
    pub port: Change,
    pub identity_file: Change,
}
impl OptionChanges { pub fn get(&self, key: OptionKey) -> &Change; }

/// A host block to append. `None` options are omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHost {
    pub name: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<String>,
    pub identity_file: Option<String>,
}

/// An edit of one alias and the options of its block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEdit {
    pub block_index: usize,
    pub pattern_index: usize,
    /// New alias for the pattern at `pattern_index`; `None` keeps it.
    pub new_name: Option<String>,
    pub changes: OptionChanges,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("block {0} does not exist")]
    BlockNotFound(usize),
    #[error("block {0} is not a Host block")]
    NotHostBlock(usize),
    #[error("pattern {pattern_index} does not exist in block {block_index}")]
    PatternNotFound { block_index: usize, pattern_index: usize },
    #[error("{0:?} cannot be edited because the block defines it in a form the editor cannot represent")]
    Unrepresentable(OptionKey),
    #[error("invalid value: {0}")]
    InvalidValue(String),
    #[error(transparent)]
    InvalidName(#[from] ValidationError),
}

/// Formats one argument for writing. See the rules below.
pub fn format_arg(value: &str) -> Result<String, EditError>;

/// Reads the four options of a Host block.
pub fn block_fields(parsed: &ParsedConfig, block_index: usize) -> Result<BlockFields, EditError>;

/// Returns the file with a new Host block inserted.
pub fn add_host(parsed: &ParsedConfig, host: &NewHost) -> Result<Vec<u8>, EditError>;

/// Returns the file with the rename and option changes applied.
pub fn apply_host_edit(parsed: &ParsedConfig, edit: &HostEdit) -> Result<Vec<u8>, EditError>;

/// Returns the file with one alias removed (the whole block when it is the only pattern).
pub fn delete_pattern(
    parsed: &ParsedConfig,
    block_index: usize,
    pattern_index: usize,
) -> Result<Vec<u8>, EditError>;
```

오류가 나면 부분 적용 없이 `Err`만 반환한다. 변경이 전혀 없는 요청은 원본과 같은 바이트를 반환한다.

## format_arg 규칙

- 빈 문자열, 제어 문자(`char::is_control`) 포함, `"` 포함은 `InvalidValue`.
- 다음 중 하나라도 해당하면 값을 큰따옴표로 감싼다: 공백 또는 탭 포함, `#` 포함, `'` 포함, `=`로 시작.
- 백슬래시 이스케이프: 값 안의 백슬래시 중 바로 뒤 문자가 `\`, `'`인 것은 `\\`로 쓴다. 따옴표로 감싸는 경우 값의 마지막 문자가 백슬래시이면 그것도 `\\`로 쓴다(닫는 따옴표가 이스케이프되는 것 방지). 그 외 백슬래시는 그대로 쓴다.
- 성질: `InvalidValue`가 아닌 모든 `v`에 대해 `lex_line(format!("K {}", format_arg(v)))`의 인자는 정확히 1개이고 그 값은 `v`다.

`Host` 줄의 별칭, 옵션 값 모두 이 함수로 만든다.

## 공통 정의

- 본문 줄: 블록의 `header + 1 .. end` 범위의 줄.
- 옵션 줄: 본문 줄 중 `Directive::is(key.keyword())`인 줄.
- 파일의 줄바꿈: 종결자가 있는 첫 줄의 `Eol`. 그런 줄이 없으면 LF.
- 들여쓰기(바이트열): 줄 내용의 시작부터 키워드 시작 전까지의 바이트.
  - 블록 들여쓰기: 그 블록의 첫 번째 본문 `Directive` 줄의 들여쓰기. 본문에 `Directive` 줄이 없으면 파일 들여쓰기.
  - 파일 들여쓰기: 파일 전체에서, 어떤 블록의 본문에 속하면서 들여쓰기가 비어 있지 않은 첫 번째 `Directive` 줄의 들여쓰기. 없으면 공백 4개.
- 붙은 주석 묶음: 어떤 헤더 줄 바로 위에 빈 줄 없이 연속으로 있는 `Comment` 줄들 가운데 들여쓰기가 없는(줄 내용의 첫 바이트가 `#`인) 줄들. 헤더에서 위로 올라가며 조건에 맞지 않는 줄을 만나면 멈춘다.

## block_fields

- `block_index`가 범위 밖이면 `BlockNotFound`, `Match` 블록이면 `NotHostBlock`.
- 키마다 옵션 줄을 모아 `FieldState`를 정한다. `Unrepresentable::lines`는 해당 줄들의 내용(종결자 제외)을 순서대로 담는다.

## add_host

1. `validate_nickname(&host.name)` 실패는 `InvalidName`. 값들은 `format_arg`로 변환(실패는 `InvalidValue`). `Some("")`은 `InvalidValue`.
2. 새 블록 텍스트: `Host <name><eol>` 다음에 `Some`인 옵션을 정해진 순서(HostName, User, Port, IdentityFile)로 `<파일 들여쓰기><Keyword> <value><eol>`.
3. 삽입 위치: 파일에서 첫 번째 "가리는 블록"의 앞. 가리는 블록은 `Match` 블록, 또는 패턴 중 하나라도 `is_listable_pattern`이 아닌 `Host` 블록이다. 그 블록에 붙은 주석 묶음이 있으면 묶음의 첫 줄 앞에 넣는다.
   - 삽입 지점 바로 윗줄이 있고 `Blank`가 아니면 새 블록 앞에 빈 줄(`<eol>`) 하나를 넣는다.
   - 새 블록 뒤에 빈 줄(`<eol>`) 하나를 넣는다.
4. 가리는 블록이 없으면 파일 끝에 추가한다.
   - 빈 파일: 새 블록만.
   - 마지막 줄에 종결자가 없으면 먼저 `<eol>`을 붙인다.
   - 마지막 줄이 `Blank`가 아니면 빈 줄(`<eol>`) 하나를 넣고 새 블록을 붙인다. 새 블록 뒤에는 빈 줄을 넣지 않는다.

## apply_host_edit

- 대상 블록 검증: `BlockNotFound`, `NotHostBlock`, `PatternNotFound`.
- 이름 변경: `new_name`이 `Some`이고 현재 값과 다르면 `validate_nickname` 후 헤더의 `args[pattern_index].span`을 `format_arg(new_name)`으로 대체한다. 같은 줄의 다른 패턴, 공백, 후행 주석은 그대로다.
- 옵션마다 `changes.get(key)`와 현재 `FieldState`에 따라 처리한다.
  - `Keep`: 변경 없음.
  - `Set(v)`, 현재 `Value(cur)`: `v == cur`이면 변경 없음. 아니면 그 옵션 줄의 `args[0].span`만 `format_arg(v)`로 대체.
  - `Set(v)`, 현재 `Absent`: 새 줄 `<블록 들여쓰기><Keyword> <format_arg(v)>`를 기준 줄 뒤에 삽입. 기준 줄은 블록 본문의 마지막 `Directive` 줄이고, 없으면 헤더 줄이다. 기준 줄에 종결자가 있으면 그 `full_span` 끝에 `<새 줄><eol>`을 넣고, 종결자가 없으면(파일의 마지막 줄) 줄 끝에 `<eol><새 줄>`을 넣는다. 한 번에 여러 옵션을 추가하면 같은 기준 줄 뒤에 정해진 순서로 넣는다.
  - `Remove`, 현재 `Value`: 그 줄의 `full_span` 삭제.
  - `Remove`, 현재 `Absent`: 변경 없음.
  - `Set` 또는 `Remove`, 현재 `Unrepresentable`: `Err(Unrepresentable(key))`.
  - `Set("")`은 `InvalidValue`.
- 모든 검증을 먼저 끝낸 뒤 대체 목록을 한 번에 적용한다.

## delete_pattern

- 대상 블록 검증은 위와 같다.
- 헤더의 패턴이 2개 이상이면 그 패턴 토큰과 인접 공백만 지운다.
  - 마지막 패턴이 아니면 `args[i].span.start .. args[i + 1].span.start` 삭제.
  - 마지막 패턴이면 `args[i - 1].span.end .. args[i].span.end` 삭제.
- 패턴이 1개이면 블록 전체를 지운다.
  - 삭제 줄 범위는 `header .. stop`. 다음 블록이 있으면 `stop`은 다음 블록에 붙은 주석 묶음의 첫 줄(묶음이 없으면 다음 헤더)이다. 다음 블록이 없으면 `stop`은 줄 수(파일 끝)다.
  - 다음 블록이 없는 경우에 한해 헤더 바로 위에 연속으로 있는 `Blank` 줄들도 함께 지운다.
  - 지우는 블록 위의 주석 줄은 지우지 않는다.

## 테스트

기대값은 전체 바이트 문자열의 완전 일치(`assert_eq!`)로 검증한다. 일부만 확인하는 `contains` 방식 단언은 쓰지 않는다. 아래 기준 입력 `BASE`를 테스트 상수로 둔다(둘째 블록의 들여쓰기는 탭 문자, 나머지는 공백 4개).

```
# Global settings
ServerAliveInterval 60

# Web server
Host web
    HostName web.example.com   # primary
    user = alice
    Port=2222
    ForwardAgent yes

Host db cache
<TAB>HostName 10.0.0.5
<TAB>IdentityFile "~/.ssh/my key"

# Defaults for everything
Host *
    User root
```

마지막 줄 `    User root` 뒤에는 `\n`이 있다.

`apply_host_edit` (블록 0은 `web`, 블록 1은 `db cache`):

1. `web`의 HostName을 `web2.example.com`으로: 해당 줄이 `    HostName web2.example.com   # primary`가 되고 나머지 동일
2. `web`의 User를 `bob`으로: `    user = bob`
3. `web`의 Port 제거: `    Port=2222` 줄만 사라짐
4. `web`에 IdentityFile `~/.ssh/id_ed25519` 추가: `    ForwardAgent yes` 줄 바로 뒤에 `    IdentityFile ~/.ssh/id_ed25519` 줄 삽입
5. `db cache`의 IdentityFile을 `/keys/new key`로: `<TAB>IdentityFile "/keys/new key"`
6. `db cache`에 User `x`와 Port `2200` 동시 추가: IdentityFile 줄 뒤에 `<TAB>User x`, `<TAB>Port 2200` 순서
7. 1, 2, 3, 4를 한 번의 요청으로 적용한 결과
8. 변경 없는 요청(모두 `Keep`, `new_name` 없음 또는 현재 이름과 동일)은 `BASE`와 동일
9. `Set`의 값이 현재 값과 같으면 원문 유지: `db cache`의 IdentityFile을 `~/.ssh/my key`로 설정해도 `BASE`와 동일
10. 이름 변경: `db`를 `database`로 하면 `Host database cache`, `cache`를 `redis`로 하면 `Host db redis`. 본문은 동일
11. 이름 변경과 옵션 변경 동시 적용
12. 본문이 없고 종결자도 없는 파일 `Host a`에 HostName `h` 추가: `Host a\n    HostName h`
13. 종결자 없는 마지막 줄 제거: `Host a\n    Port 22`에서 Port 제거하면 `Host a\n`
14. 소문자 키워드 `hostname old` 줄의 값 변경 시 키워드 표기 유지
15. `IdentityFile` 줄이 2개인 블록: `block_fields`가 `Unrepresentable`(줄 2개 수록), `Set`과 `Remove`는 `Err(Unrepresentable(IdentityFile))`, 같은 블록의 User 변경은 성공하고 IdentityFile 두 줄은 그대로
16. `HostName a b`(인자 2개)와 `HostName`(인자 0개)은 `Unrepresentable`
17. 다른 블록에 같은 키워드가 있어도 대상 블록만 변경
18. 오류: 범위 밖 블록, `Match` 블록, 범위 밖 패턴, `Set("")`, 제어 문자가 든 값, `"`가 든 값, 잘못된 새 이름(`a b`, `-x`, `a*`)
19. UTF-8이 아닌 바이트(`0xff`)가 주석에 든 파일에서 옵션 변경 후 그 바이트 보존
20. CRLF 파일(`BASE`의 모든 `\n`을 `\r\n`으로 바꾼 것): 1, 3, 4, 6을 적용한 결과가 LF 기대값의 `\n`을 `\r\n`으로 바꾼 것과 동일

`add_host`:

21. `BASE`에 `{ name: "new", host_name: "h.example.com", port: "22" }` 추가: `<TAB>IdentityFile "~/.ssh/my key"` 뒤의 빈 줄 다음, `# Defaults for everything` 앞에 아래가 들어감(뒤에 빈 줄 하나)

```
Host new
    HostName h.example.com
    Port 22

```

22. 옵션이 전부 `Some`인 경우의 순서와, 공백이 든 IdentityFile의 따옴표 처리
23. 가리는 블록이 없는 파일(`Host a\n    HostName x\n`)은 끝에 빈 줄 하나를 두고 추가
24. 빈 파일
25. 마지막 줄에 종결자가 없는 파일(`Host a\n    HostName x`): `Host a\n    HostName x\n\nHost new\n    HostName h\n`
26. 파일이 빈 줄로 끝나는 경우(`Host a\n\n`)에는 빈 줄을 더 넣지 않음
27. `Match` 블록이 첫 가리는 블록인 경우 그 앞에 삽입
28. `Host web *.internal`처럼 와일드카드가 섞인 블록, `Host !x y` 블록도 가리는 블록으로 취급
29. 가리는 블록 바로 위 줄이 `Blank`가 아닌 경우(`Host a\n    HostName x\nHost *\n`) 새 블록 앞뒤에 빈 줄 하나씩
30. 붙은 주석 묶음이 2줄인 경우 그 앞에 삽입. 들여쓴 주석(앞 블록 본문의 주석)은 묶음에 넣지 않음
31. 탭 들여쓰기 파일에서는 새 블록도 탭 들여쓰기
32. CRLF 파일에 추가하면 새 줄 전부 CRLF
33. 오류: 잘못된 이름, `Some("")`, 제어 문자가 든 값

`delete_pattern`:

34. `BASE`에서 `db` 삭제: `Host cache`. `cache` 삭제: `Host db`
35. 패턴 3개(`Host a  b   c # note`)에서 각각 삭제한 결과 3종(남은 공백과 후행 주석 포함 완전 일치)
36. `BASE`에서 `web` 삭제: `Host web`부터 `    ForwardAgent yes` 뒤의 빈 줄까지 사라지고 `# Web server` 줄은 남음
37. 마지막 블록 삭제 시 헤더 위의 빈 줄도 삭제: `Host a\n    HostName x\n\nHost b\n    HostName y\n`에서 `b` 삭제 결과 `Host a\n    HostName x\n`
38. 다음 블록에 붙은 주석 묶음은 보존, 지우는 블록 본문의 들여쓴 주석은 삭제
39. 파일에 블록이 하나뿐인 경우 삭제 후 전역 옵션만 남음
40. CRLF 파일에서 36과 같은 삭제

`format_arg`와 `Change::from_form`:

41. 따옴표가 필요 없는 값, 공백, 탭, `#`, `'`, `=` 시작 값
42. 백슬래시: `C:\Users\me\key`(그대로), `\\server\share`, `it's`, `dir with space\`(끝 백슬래시)
43. 왕복 성질: 41과 42의 모든 값과 `a\'b`, `a\\\\b`, `한글 경로/키`에 대해 `lex_line` 결과 인자 1개, 값 일치
44. `from_form`의 6가지 경우

## 추가 테스트 1건 (src/config/reader.rs)

45. ssh 디렉터리 이름이 `.ssh`인 경우(`<tmp>/home/.ssh`를 ssh 디렉터리로 주입) `Include conf.d/*.conf`가 `<tmp>/home/.ssh/conf.d/a.conf`를 찾음. 이 테스트가 실패하면 구현을 고치지 말고 최종 보고에 원인과 함께 적는다.

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- 위 테스트가 모두 존재하고 기존 테스트 59개 유지
