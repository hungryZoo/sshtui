# 작업 4 재지시 1: 삭제되는 줄을 삽입 기준으로 쓰는 문제

먼저 저장소 루트의 `AGENTS.md`를 읽고 따른다. 작업 4(`tasks/04_config_edit.md`)의 결과물에 대한 수정이다.

## 대상 파일

- `src/config/edit.rs`

## 문제 1: 삭제되는 옵션 줄이 삽입 기준 줄로 선택됨

위치: `apply_host_edit`의 `anchor` 계산(`body.iter().rfind(|line| line.directive().is_some())`).

같은 요청에서 `Remove`로 지워지는 줄이 본문의 마지막 `Directive` 줄이면 그 줄이 삽입 기준이 된다. 그 줄에 종결자가 없을 때(파일의 마지막 줄) 결과에 빈 줄이 생긴다.

재현:

- 입력: `Host a\n    Port 22` (마지막 줄에 종결자 없음)
- 요청: Port `Remove`, HostName `Set("h")`
- 현재 결과: `Host a\n\n    HostName h`
- 기대 결과: `Host a\n    HostName h\n`

기대 동작: 삽입 기준 줄은 "같은 요청에서 삭제되지 않는" 본문의 마지막 `Directive` 줄이다. 그런 줄이 없으면 헤더 줄이다. 그 외 삽입 규칙(기준 줄에 종결자가 있으면 `full_span` 끝에 `<새 줄><eol>`, 없으면 줄 끝에 `<eol><새 줄>`)은 그대로다.

## 문제 2: 시작 위치가 같은 대체 항목의 적용 순서가 정해져 있지 않음

위치: `apply_replacements`의 `sort_unstable_by_key(|(range, _)| range.start)`.

문제 1을 고치면 삽입 지점(길이 0인 범위 `X..X`)과 바로 다음 줄의 삭제 범위(`X..Y`)가 같은 시작 위치를 가질 수 있다. 삭제가 먼저 적용되면 `bytes[end..range.start]`에서 `end > range.start`가 되어 패닉이 난다.

기대 동작:

- 정렬 키를 `(range.start, range.end)`로 하여 길이 0인 삽입이 같은 위치에서 시작하는 삭제보다 먼저 적용되게 한다.
- 대체 범위가 서로 겹치면 안 된다는 불변식을 `debug_assert!`로 확인한다(릴리스 빌드에서 패닉 없이 동작해야 하므로 `end.max(range.end)` 등으로 방어한다).

## 추가할 테스트

기대값은 전체 바이트 완전 일치로 검증한다.

1. 위 재현 사례: `Host a\n    Port 22`에서 Port 제거와 HostName `h` 추가 → `Host a\n    HostName h\n`
2. `Host a\n    User u\n    Port 22`(종결자 없음)에서 Port 제거와 HostName `h` 추가 → `Host a\n    User u\n    HostName h\n`
3. `Host a\n    User u\n    Port 22\n`에서 Port 제거와 IdentityFile `k` 추가 → `Host a\n    User u\n    IdentityFile k\n`
4. `Host a\n    Port 22\n    User u\n`에서 User 제거와 HostName `h` 추가 → `Host a\n    Port 22\n    HostName h\n`
5. `Host a\n    Port 22\n\nHost b\n    HostName y\n`에서 블록 0의 Port 제거와 User `u` 추가 → `Host a\n    User u\n\nHost b\n    HostName y\n`
6. 본문의 옵션 줄 4개를 모두 `Remove`한 결과(`Host a\n    HostName h\n    User u\n    Port 22\n    IdentityFile k\n` → `Host a\n`)
7. 2번을 CRLF로 바꾼 사례: `Host a\r\n    User u\r\n    Port 22` → `Host a\r\n    User u\r\n    HostName h\r\n`

## 완료 조건

- `AGENTS.md`의 완료 조건 3개 명령 통과
- 기존 테스트 106개 유지, 위 테스트 7개 추가
